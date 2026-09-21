//! `vfiles import`：把服务器上的本地目录导入到某个用户命名空间。
//!
//! 与 FTP 通道共用 `ImportBatch`：按文件数阈值/结束时提交快照，避免逐文件写全量快照。
//! 该命令在服务端本地读取源目录，不需要开放任何网络端口，适合首次全量灌数据。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, anyhow, bail};
use vfiles_app::{ImportBatch, ImportedDirectory, SnapshotMode};
use vfiles_domain::{EntryRepo, NamespaceRepo, NormalizedPath, SnapshotRepo, UserId, UserRepo};

/// 导入参数（由 CLI 解析后传入，便于单元测试）。
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// 源目录（本机路径）
    pub source: PathBuf,
    /// 目标目录（命名空间内路径，空字符串表示根目录）
    pub target: String,
    /// 归属用户（按用户名解析其命名空间）
    pub owner: String,
    /// 快照策略
    pub snapshot_mode: SnapshotMode,
    /// `batch` 模式的提交阈值
    pub flush_files: usize,
    /// 内容未变化的文件是否跳过（默认跳过）
    pub skip_unchanged: bool,
    /// 只统计不写入
    pub dry_run: bool,
    /// 单文件大小上限（`None` 表示不限制）
    pub max_file_size_bytes: Option<u64>,
    /// 是否包含以 `.` 开头的隐藏文件（默认包含）
    pub include_hidden: bool,
}

/// 导入过程中的计数与错误。
#[derive(Debug, Default)]
pub struct ImportReport {
    pub files: u64,
    pub directories: u64,
    pub unchanged: u64,
    pub bytes: u64,
    pub skipped_symlinks: u64,
    pub snapshots: u64,
    pub errors: Vec<String>,
}

impl ImportReport {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// 导入所需的依赖（由命令层装配，测试可注入临时目录与数据库）。
pub struct ImportDeps {
    pub workspace: Arc<vfiles_app::DefaultWorkspaceService>,
    pub entry_repo: Arc<dyn EntryRepo + Send + Sync>,
    pub snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
    pub blob_store: Arc<dyn vfiles_domain::BlobStore + Send + Sync>,
    pub user_repo: Arc<dyn UserRepo + Send + Sync>,
    pub namespace_repo: Arc<dyn NamespaceRepo + Send + Sync>,
}

/// 执行导入：递归遍历源目录，写入目标命名空间。
pub async fn run_import(deps: ImportDeps, options: ImportOptions) -> anyhow::Result<ImportReport> {
    let source = options
        .source
        .canonicalize()
        .with_context(|| format!("源目录不存在或不可访问: {}", options.source.display()))?;
    if !source.is_dir() {
        bail!("源路径不是目录: {}", source.display());
    }

    let target = NormalizedPath::new(options.target.trim_matches('/'))
        .map_err(|err| anyhow!("目标路径非法: {err}"))?;

    // 归属用户 → 命名空间（不存在则创建，与 Web/FTP 一致）
    let username =
        vfiles_domain::Username::new(&options.owner).map_err(|err| anyhow!("用户名非法: {err}"))?;
    let user = deps
        .user_repo
        .find_by_username(&username)
        .await
        .with_context(|| format!("用户不存在: {}", options.owner))?;
    let namespaces = vfiles_app::NamespaceService::new(Arc::clone(&deps.namespace_repo));
    let namespace_id = namespaces
        .ensure_default_for_owner(&user.id)
        .await
        .context("无法解析用户的默认命名空间")?;

    let mut report = ImportReport::default();
    let started = Instant::now();

    // 遍历前先统计，便于给用户一个进度基准
    let plan = collect_plan(&source, options.include_hidden)?;
    report.skipped_symlinks = plan.skipped_symlinks;
    let total_files = plan.files.len();
    println!(
        "准备导入 {} 个文件、{} 个目录 → [{}]{}",
        total_files,
        plan.directories.len(),
        user.username,
        if target.as_str().is_empty() {
            String::new()
        } else {
            format!("/{}", target.as_str())
        }
    );

    if options.dry_run {
        report.files = plan.files.len() as u64;
        report.directories = plan.directories.len() as u64;
        report.bytes = plan.bytes;
        println!(
            "预览模式：不写入任何数据（{} 字节，{} 个符号链接已跳过）",
            plan.bytes, plan.skipped_symlinks
        );
        return Ok(report);
    }

    let mut batch = ImportBatch::with_options(
        Arc::clone(&deps.entry_repo),
        Arc::clone(&deps.snapshot_repo),
        Arc::clone(&deps.blob_store),
        namespace_id,
        user.id,
        "目录导入",
        options.snapshot_mode,
        options.flush_files,
        options.skip_unchanged,
    );

    // 先建目录（含空目录），再把文件写进去
    for directory in &plan.directories {
        let path = join_target(&target, &directory.relative)?;
        match batch.create_directory(&path).await {
            Ok(ImportedDirectory::Created) => report.directories += 1,
            Ok(ImportedDirectory::Existing) => {}
            Err(err) => report
                .errors
                .push(format!("目录 {} 创建失败: {err}", directory.relative)),
        }
    }

    let mut processed = 0_usize;
    for file in &plan.files {
        let path = join_target(&target, &file.relative)?;

        let handle = match tokio::fs::File::open(&file.absolute).await {
            Ok(handle) => handle,
            Err(err) => {
                report
                    .errors
                    .push(format!("读取 {} 失败: {err}", file.absolute.display()));
                continue;
            }
        };

        match batch
            .import_file_stream(
                &path,
                Box::new(handle),
                options.max_file_size_bytes,
                Some(&format!("导入: {}", file.relative)),
            )
            .await
        {
            Ok(imported) => {
                if imported.unchanged {
                    report.unchanged += 1;
                } else {
                    report.files += 1;
                    report.bytes += imported.size_bytes;
                }
            }
            Err(err) => report
                .errors
                .push(format!("导入 {} 失败: {err}", file.relative)),
        }

        processed += 1;
        if processed.is_multiple_of(200) || processed == total_files {
            println!(
                "  进度 {processed}/{total_files}（写入 {}，跳过 {}，失败 {}）",
                report.files,
                report.unchanged,
                report.errors.len()
            );
        }
    }

    if batch.finish().await.is_ok() {
        report.snapshots = batch.snapshots_written();
    } else {
        report.errors.push("提交快照失败".to_string());
    }

    let elapsed = started.elapsed();
    println!(
        "完成：写入 {} 个文件（{} 字节）、{} 个目录，跳过未变更 {} 个，快照 {} 次，耗时 {:.1}s",
        report.files,
        report.bytes,
        report.directories,
        report.unchanged,
        report.snapshots,
        elapsed.as_secs_f64()
    );
    if !report.errors.is_empty() {
        println!("失败 {} 项：", report.errors.len());
        for error in report.errors.iter().take(20) {
            println!("  - {error}");
        }
        if report.errors.len() > 20 {
            println!("  … 其余 {} 项省略", report.errors.len() - 20);
        }
    }

    // 让编译器看到 workspace 字段的用途（保留给未来的服务端校验/配额检查）
    let _ = &deps.workspace;

    Ok(report)
}

#[derive(Debug)]
struct PlannedFile {
    relative: String,
    absolute: PathBuf,
}

#[derive(Debug)]
struct PlannedDirectory {
    relative: String,
}

#[derive(Debug, Default)]
struct ImportPlan {
    files: Vec<PlannedFile>,
    directories: Vec<PlannedDirectory>,
    bytes: u64,
    skipped_symlinks: u64,
}

/// 递归收集待导入的文件与目录（目录按层级浅到深排序，便于先建目录）。
fn collect_plan(source: &Path, include_hidden: bool) -> anyhow::Result<ImportPlan> {
    let mut plan = ImportPlan::default();
    let mut stack = vec![(source.to_path_buf(), String::new())];

    while let Some((directory, relative)) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&directory)
            .with_context(|| format!("读取目录失败: {}", directory.display()))?
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if !include_hidden && name.starts_with('.') {
                continue;
            }

            let child_relative = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            let file_type = entry.file_type()?;

            if file_type.is_symlink() {
                // 不跟随符号链接：避免导入到源目录之外的内容
                plan.skipped_symlinks += 1;
                continue;
            }

            if file_type.is_dir() {
                plan.directories.push(PlannedDirectory {
                    relative: child_relative.clone(),
                });
                stack.push((entry.path(), child_relative));
            } else if file_type.is_file() {
                let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                plan.bytes += size;
                plan.files.push(PlannedFile {
                    relative: child_relative,
                    absolute: entry.path(),
                });
            }
        }
    }

    // 目录浅到深：父目录先创建
    plan.directories
        .sort_by_key(|directory| directory.relative.matches('/').count());
    plan.files.sort_by(|a, b| a.relative.cmp(&b.relative));

    Ok(plan)
}

fn join_target(target: &NormalizedPath, relative: &str) -> anyhow::Result<NormalizedPath> {
    let joined = if target.as_str().is_empty() {
        relative.to_string()
    } else {
        format!("{}/{}", target.as_str(), relative)
    };
    NormalizedPath::new(&joined).map_err(|err| anyhow!("目标路径非法（{joined}）: {err}"))
}

/// 供命令层复用的默认归属用户解析（默认取默认命名空间的 owner）。
pub async fn resolve_default_owner(
    user_repo: &(dyn UserRepo + Send + Sync),
    namespace_repo: &(dyn NamespaceRepo + Send + Sync),
) -> anyhow::Result<(UserId, String)> {
    let namespace_id = namespace_repo
        .find_default()
        .await
        .context("找不到默认命名空间，请先执行 `vfiles init`")?;

    // 命名空间表里只有 owner_user_id；这里通过列出用户找到匹配项
    let users = user_repo
        .list_users(1000, 0)
        .await
        .context("无法列出用户")?;
    for user in users {
        if let Ok(owner_namespace) = namespace_repo.find_default_for_owner(&user.id).await
            && owner_namespace == namespace_id
        {
            return Ok((user.id, user.username.as_str().to_string()));
        }
    }

    Err(anyhow!(
        "默认命名空间没有对应所有者，请用 --owner 指定用户名"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use vfiles_domain::{NamespaceRepo, Role};
    use vfiles_infra_sqlite::{
        FsBlobStore, FsUploadStore, SqliteEntryRepo, SqliteMigrations, SqliteNamespaceRepo,
        SqlitePoolFactory, SqliteSnapshotRepo, SqliteUserRepo,
    };

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, contents).expect("write file");
    }

    #[test]
    fn plan_walks_directories_skips_symlinks_and_orders_directories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path();
        write(&source.join("docs/2026/nested/a.txt"), "aaa");
        write(&source.join("docs/b.txt"), "bb");
        write(&source.join(".hidden"), "h");
        std::fs::create_dir_all(source.join("empty")).expect("create empty dir");

        #[cfg(unix)]
        std::os::unix::fs::symlink(source.join("docs/b.txt"), source.join("link"))
            .expect("symlink");

        let plan = collect_plan(source, true).expect("plan should build");
        let files: Vec<_> = plan
            .files
            .iter()
            .map(|file| file.relative.clone())
            .collect();
        assert_eq!(
            files,
            vec![".hidden", "docs/2026/nested/a.txt", "docs/b.txt"]
        );
        assert_eq!(plan.bytes, 6, "字节数应为全部文件之和");

        let directories: Vec<_> = plan
            .directories
            .iter()
            .map(|dir| dir.relative.clone())
            .collect();
        assert_eq!(
            directories,
            vec!["docs", "empty", "docs/2026", "docs/2026/nested"]
        );
        // 父目录必须排在子目录之前
        let depth = |value: &str| value.matches('/').count();
        for pair in directories.windows(2) {
            assert!(
                depth(&pair[0]) <= depth(&pair[1]),
                "目录应按层级排列: {directories:?}"
            );
        }

        #[cfg(unix)]
        assert_eq!(plan.skipped_symlinks, 1, "符号链接应被跳过");

        // 排除隐藏文件
        let plan = collect_plan(source, false).expect("plan should build");
        assert!(
            !plan.files.iter().any(|file| file.relative == ".hidden"),
            "exclude_hidden 应跳过点文件"
        );
    }

    struct Fixture {
        _temp_dir: tempfile::TempDir,
        pool: sqlx::SqlitePool,
        storage_root: Utf8PathBuf,
        owner: String,
        entry_repo: Arc<dyn EntryRepo + Send + Sync>,
        snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
        blob_store: Arc<dyn vfiles_domain::BlobStore + Send + Sync>,
        user_repo: Arc<dyn UserRepo + Send + Sync>,
        namespace_repo: Arc<dyn NamespaceRepo + Send + Sync>,
        workspace: Arc<vfiles_app::DefaultWorkspaceService>,
    }

    impl Fixture {
        async fn new() -> Self {
            let temp_dir = tempfile::tempdir().expect("tempdir");
            let storage_root =
                Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf8");
            let pool = SqlitePoolFactory::connect(storage_root.join("vfiles.db").as_path())
                .await
                .expect("pool should connect");
            SqliteMigrations::run(&pool).await.expect("migrations");

            let user_repo = SqliteUserRepo::new(pool.clone());
            let user_id = user_repo
                .create_admin("admin", "admin@example.com", "hash")
                .await
                .expect("create admin");
            let namespace_id = SqliteNamespaceRepo::new(pool.clone())
                .create_default(&user_id, "default")
                .await
                .expect("create namespace");

            let workspace = Arc::new(vfiles_app::DefaultWorkspaceService::new(
                SqliteEntryRepo::new(pool.clone()),
                SqliteSnapshotRepo::new(pool.clone()),
                FsBlobStore::new(pool.clone(), storage_root.join("blobs")),
                FsUploadStore::new(storage_root.join("uploads")),
            ));

            let _ = namespace_id;
            Self {
                _temp_dir: temp_dir,
                pool: pool.clone(),
                storage_root: storage_root.clone(),
                owner: "admin".to_string(),
                entry_repo: Arc::new(SqliteEntryRepo::new(pool.clone())),
                snapshot_repo: Arc::new(SqliteSnapshotRepo::new(pool.clone())),
                blob_store: Arc::new(FsBlobStore::new(pool.clone(), storage_root.join("blobs"))),
                user_repo: Arc::new(SqliteUserRepo::new(pool.clone())),
                namespace_repo: Arc::new(SqliteNamespaceRepo::new(pool.clone())),
                workspace,
            }
        }

        fn deps(&self) -> ImportDeps {
            ImportDeps {
                workspace: Arc::clone(&self.workspace),
                entry_repo: Arc::clone(&self.entry_repo),
                snapshot_repo: Arc::clone(&self.snapshot_repo),
                blob_store: Arc::clone(&self.blob_store),
                user_repo: Arc::clone(&self.user_repo),
                namespace_repo: Arc::clone(&self.namespace_repo),
            }
        }

        fn options(&self, source: &Path) -> ImportOptions {
            ImportOptions {
                source: source.to_path_buf(),
                target: "imported".to_string(),
                owner: self.owner.clone(),
                snapshot_mode: SnapshotMode::Batch,
                flush_files: 100,
                skip_unchanged: true,
                dry_run: false,
                max_file_size_bytes: Some(1024 * 1024),
                include_hidden: true,
            }
        }

        async fn snapshot_count(&self) -> i64 {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM snapshots")
                .fetch_one(&self.pool)
                .await
                .expect("snapshot count")
        }

        async fn entry_paths(&self) -> Vec<String> {
            let mut entries = self
                .entry_repo
                .find_all(
                    &self
                        .namespace_repo
                        .find_default()
                        .await
                        .expect("default namespace"),
                )
                .await
                .expect("find_all");
            entries.sort_by(|a, b| a.path_norm.as_str().cmp(b.path_norm.as_str()));
            entries
                .into_iter()
                .map(|entry| entry.path_norm.as_str().to_string())
                .collect()
        }

        async fn version_count(&self) -> i64 {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM entry_versions")
                .fetch_one(&self.pool)
                .await
                .expect("version count")
        }

        fn _touch(&self) {
            let _ = Role::Admin;
        }
    }

    #[tokio::test]
    async fn imports_a_directory_tree_and_is_idempotent() {
        let fixture = Fixture::new().await;
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path();
        write(&source.join("docs/a.txt"), "aaa");
        write(&source.join("docs/b.txt"), "bbb");
        std::fs::create_dir_all(source.join("docs/empty")).expect("empty dir");

        let report = run_import(fixture.deps(), fixture.options(source))
            .await
            .expect("import should succeed");
        assert_eq!(report.files, 2);
        assert_eq!(report.directories, 2, "docs 与 docs/empty 都算新建目录");
        assert_eq!(report.bytes, 6);
        assert!(!report.has_errors());
        assert_eq!(fixture.snapshot_count().await, 1, "整个导入只提交一次快照");

        assert_eq!(
            fixture.entry_paths().await,
            vec![
                "imported",
                "imported/docs",
                "imported/docs/a.txt",
                "imported/docs/b.txt",
                "imported/docs/empty",
            ]
        );
        assert_eq!(fixture.version_count().await, 2);

        // 二次导入：内容未变化 ⇒ 不写版本、不提交快照
        let report = run_import(fixture.deps(), fixture.options(source))
            .await
            .expect("re-import should succeed");
        assert_eq!(report.files, 0);
        assert_eq!(report.unchanged, 2);
        assert_eq!(fixture.snapshot_count().await, 1, "未变更时不应新增快照");
        assert_eq!(fixture.version_count().await, 2);

        // --force：即使未变化也生成新版本
        let mut forced = fixture.options(source);
        forced.skip_unchanged = false;
        let report = run_import(fixture.deps(), forced)
            .await
            .expect("forced import should succeed");
        assert_eq!(report.files, 2);
        assert_eq!(fixture.version_count().await, 4);
    }

    #[tokio::test]
    async fn dry_run_writes_nothing() {
        let fixture = Fixture::new().await;
        let temp = tempfile::tempdir().expect("tempdir");
        write(&temp.path().join("only.txt"), "x");

        let mut options = fixture.options(temp.path());
        options.dry_run = true;
        let report = run_import(fixture.deps(), options)
            .await
            .expect("dry run should succeed");

        assert_eq!(report.files, 1);
        assert!(
            fixture.entry_paths().await.is_empty(),
            "预览模式不应写入条目"
        );
        assert_eq!(fixture.snapshot_count().await, 0);

        // storage_root 被测试夹具持有，避免提前回收
        assert!(fixture.storage_root.join("vfiles.db").exists());
    }

    #[tokio::test]
    async fn missing_source_and_unknown_owner_fail_loudly() {
        let fixture = Fixture::new().await;

        let mut options = fixture.options(Path::new("/definitely/not/here"));
        let err = run_import(fixture.deps(), options.clone())
            .await
            .expect_err("missing source must fail");
        assert!(err.to_string().contains("源目录不存在"), "实际: {err}");

        let temp = tempfile::tempdir().expect("tempdir");
        write(&temp.path().join("a.txt"), "x");
        options = fixture.options(temp.path());
        options.owner = "nobody".to_string();
        let err = run_import(fixture.deps(), options)
            .await
            .expect_err("unknown owner must fail");
        assert!(err.to_string().contains("用户不存在"), "实际: {err}");
    }
}
