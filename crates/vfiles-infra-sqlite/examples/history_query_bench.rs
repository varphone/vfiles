//! 快照/历史读路径的并发基准。
//!
//! 用来复核「查询代价应与返回数据量相关」这一约束，而不是随命名空间、快照或
//! 历史的规模增长。它直接连一个已有的 VFiles 数据库，不改写任何数据。
//!
//! 用法：
//! ```text
//! cargo run --release --example history_query_bench -p vfiles-infra-sqlite -- \
//!     <db> <namespace-id> <path> [workers] [rounds]
//! ```
//!
//! - `directory history page`：目录历史的最新一页（不统计总数）；
//! - `snapshot children page`：最新快照里该目录的首页与总条目数。
//!
//! 两段都按 `workers` 个并发任务各跑 `rounds` 次，输出 p50/p95 与吞吐。

use std::time::{Duration, Instant};

use vfiles_infra_sqlite::{SqlitePool, SqlitePoolFactory};

const HISTORY_PAGE_SQL: &str = r#"
SELECT id, namespace_id, name, description, created_at, created_by
FROM snapshots s
WHERE s.namespace_id = ?
  AND (? = '' OR EXISTS (
      SELECT 1 FROM snapshot_entries se
      WHERE se.snapshot_id = s.id AND se.entry_path = ?
  ) OR EXISTS (
      SELECT 1 FROM snapshot_entries se
      WHERE se.snapshot_id = s.id AND se.entry_path >= ? AND se.entry_path < ?
  ))
ORDER BY s.created_at DESC, s.id DESC
LIMIT 50
"#;

const HISTORY_COUNT_SQL: &str = r#"
SELECT COUNT(*)
FROM snapshots s
WHERE s.namespace_id = ?
  AND (? = '' OR EXISTS (
      SELECT 1 FROM snapshot_entries se
      WHERE se.snapshot_id = s.id AND se.entry_path = ?
  ) OR EXISTS (
      SELECT 1 FROM snapshot_entries se
      WHERE se.snapshot_id = s.id AND se.entry_path >= ? AND se.entry_path < ?
  ))
"#;

const SNAPSHOT_CHILDREN_SQL: &str = r#"
SELECT entry_path, entry_kind, COUNT(*) OVER () AS total
FROM snapshot_entries
WHERE snapshot_id = ? AND parent_path = ? AND change_type != 'deleted'
ORDER BY (entry_kind = 'directory') DESC, entry_path ASC
LIMIT 200 OFFSET 0
"#;

const SNAPSHOT_CHILDREN_PLAN_SQL: &str = r#"
EXPLAIN QUERY PLAN
SELECT entry_path, entry_kind, COUNT(*) OVER () AS total
FROM snapshot_entries
WHERE snapshot_id = ? AND parent_path = ? AND change_type != 'deleted'
ORDER BY (entry_kind = 'directory') DESC, entry_path ASC
LIMIT 200 OFFSET 0
"#;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let db = args
        .get(1)
        .expect("usage: <db> <namespace> <path> [workers] [rounds]")
        .clone();
    let namespace = args.get(2).expect("namespace id").clone();
    let path = args.get(3).cloned().unwrap_or_default();
    let workers: usize = args
        .get(4)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8);
    let rounds: usize = args
        .get(5)
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);

    let pool = open_pool(&db).await;
    let snapshot_id: String = sqlx::query_scalar(
        "SELECT id FROM snapshots WHERE namespace_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&namespace)
    .fetch_one(&pool)
    .await
    .expect("namespace should have a snapshot");

    let lower = format!("{path}/");
    let upper = format!("{path}0");
    let history_namespace = namespace.clone();
    let history_path = path.clone();
    let history_lower = lower.clone();
    let history_upper = upper.clone();
    let history = benchmark(&pool, workers, rounds, move |pool| {
        let namespace = history_namespace.clone();
        let path = history_path.clone();
        let lower = history_lower.clone();
        let upper = history_upper.clone();
        Box::pin(async move {
            let _: Vec<(String, String, String, Option<String>, String, String)> =
                sqlx::query_as(HISTORY_PAGE_SQL)
                    .bind(&namespace)
                    .bind(&path)
                    .bind(&path)
                    .bind(&lower)
                    .bind(&upper)
                    .fetch_all(&pool)
                    .await
                    .expect("history page");
        })
    })
    .await;
    report("directory history page", workers, rounds, history);

    // The whole-history count is the query shape that collapses under
    // in-process concurrency; keep it in the harness to compare pragmas.
    let count_namespace = namespace.clone();
    let count_path = path.clone();
    let count_lower = format!("{path}/");
    let count_upper = format!("{path}0");
    let history_count = benchmark(&pool, workers, rounds, move |pool| {
        let namespace = count_namespace.clone();
        let path = count_path.clone();
        let lower = count_lower.clone();
        let upper = count_upper.clone();
        Box::pin(async move {
            let _: i64 = sqlx::query_scalar(HISTORY_COUNT_SQL)
                .bind(&namespace)
                .bind(&path)
                .bind(&path)
                .bind(&lower)
                .bind(&upper)
                .fetch_one(&pool)
                .await
                .expect("history count");
        })
    })
    .await;
    report("history total count", workers, rounds, history_count);

    let children_snapshot = snapshot_id.clone();
    let children_path = path.clone();
    let children = benchmark(&pool, workers, rounds, move |pool| {
        let snapshot = children_snapshot.clone();
        let path = children_path.clone();
        Box::pin(async move {
            let _: Vec<(String, String, i64)> = sqlx::query_as(SNAPSHOT_CHILDREN_SQL)
                .bind(&snapshot)
                .bind(&path)
                .fetch_all(&pool)
                .await
                .expect("snapshot children page");
        })
    })
    .await;
    report("snapshot children page", workers, rounds, children);

    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(SNAPSHOT_CHILDREN_PLAN_SQL)
        .bind(&snapshot_id)
        .bind(&path)
        .fetch_all(&pool)
        .await
        .expect("query plan should be available");
    for row in plan {
        println!("plan: {}", row.3);
    }
}

async fn benchmark<F>(pool: &SqlitePool, workers: usize, rounds: usize, work: F) -> Vec<Duration>
where
    F: Fn(SqlitePool) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync
        + 'static,
{
    let work = std::sync::Arc::new(work);
    let mut tasks = Vec::with_capacity(workers);
    for _ in 0..workers {
        let pool = pool.clone();
        let work = std::sync::Arc::clone(&work);
        tasks.push(tokio::spawn(async move {
            let mut times = Vec::with_capacity(rounds);
            for _ in 0..rounds {
                let begin = Instant::now();
                work(pool.clone()).await;
                times.push(begin.elapsed());
            }
            times
        }));
    }
    let mut all = Vec::new();
    for task in tasks {
        all.extend(task.await.expect("worker"));
    }
    all.sort();
    all
}

fn report(name: &str, workers: usize, rounds: usize, times: Vec<Duration>) {
    let p50 = times[times.len() / 2];
    let p95 = times[(times.len() as f64 * 0.95) as usize];
    println!(
        "{name:26} workers={workers:2} rounds={rounds:3} requests={:4} p50={:9.2?} p95={:9.2?} max={:9.2?}",
        times.len(),
        p50,
        p95,
        times.last().copied().unwrap_or_default()
    );
}

/// Pool with optional connection pragmas for diagnostics:
/// BENCH_MMAP (bytes), BENCH_CACHE (pages, negative = KiB), BENCH_JOURNAL
/// (wal/delete/memory). Without any of them the production factory is used.
async fn open_pool(db: &str) -> SqlitePool {
    use sqlx::sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
    };
    use std::str::FromStr;

    let mmap = std::env::var("BENCH_MMAP").ok();
    let cache = std::env::var("BENCH_CACHE").ok();
    let journal = std::env::var("BENCH_JOURNAL").ok();
    if mmap.is_none() && cache.is_none() && journal.is_none() {
        return SqlitePoolFactory::connect(&camino::Utf8PathBuf::from(db))
            .await
            .expect("database should open");
    }

    let mut options = SqliteConnectOptions::from_str(&format!("sqlite://{db}"))
        .expect("options")
        .foreign_keys(true)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));
    options = match journal.as_deref() {
        Some("delete") => options.journal_mode(SqliteJournalMode::Delete),
        Some("memory") => options.journal_mode(SqliteJournalMode::Memory),
        Some("off") => options.journal_mode(SqliteJournalMode::Off),
        _ => options.journal_mode(SqliteJournalMode::Wal),
    };
    if let Some(value) = mmap {
        options = options.pragma("mmap_size", value);
    }
    if let Some(value) = cache {
        options = options.pragma("cache_size", value);
    }
    SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await
        .expect("diagnostic pool should connect")
}
