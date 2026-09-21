//! 端到端集成测试：真实 TCP + 真实 FTP 客户端（suppaftp）。
//!
//! 覆盖登录、目录创建、批量上传、下载、重命名、删除，以及
//! 「批量上传只提交一次快照」这一批量导入的关键行为。

use std::sync::Arc;

use camino::Utf8PathBuf;
use suppaftp::FtpStream;
use tokio::sync::watch;
use vfiles_app::{
    AuthService, DefaultWorkspaceService, IngestStats, LoginAttemptLimiter, NamespaceService,
    RateLimitPolicy, SnapshotMode,
};
use vfiles_domain::{DomainResult, EntryRepo, NamespaceRepo, NormalizedPath, Role, UserRepo};
use vfiles_ftp::{
    BackendDeps, FtpApplication, FtpSettings, RoleFilter, VfilesAuthenticator,
    VfilesUserDetailProvider, spawn_ftp_server,
};
use vfiles_infra_sqlite::{
    FsBlobStore, FsUploadStore, SqliteEntryRepo, SqliteMigrations, SqliteNamespaceRepo,
    SqlitePoolFactory, SqliteSessionRepo, SqliteSnapshotRepo, SqliteUserRepo,
};

const USERNAME: &str = "ftpuser";
const PASSWORD: &str = "ftp-password-123456";

struct Harness {
    _temp_dir: tempfile::TempDir,
    pool: sqlx::SqlitePool,
    namespace_id: vfiles_domain::NamespaceId,
    entry_repo: SqliteEntryRepo,
    workspace: Arc<DefaultWorkspaceService>,
    _shutdown: watch::Sender<bool>,
    handle: vfiles_ftp::FtpServerHandle,
}

impl Harness {
    async fn start(snapshot_mode: SnapshotMode, flush_threshold: usize) -> Self {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let storage_root =
            Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf8 tempdir");
        let pool = SqlitePoolFactory::connect(storage_root.join("vfiles.db").as_path())
            .await
            .expect("pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should succeed");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let user_id = user_repo
            .create_admin(USERNAME, "ftp@example.com", "legacy-placeholder")
            .await
            .expect("user should be created");
        // 设置真实口令（走与 HTTP 相同的 argon2 哈希）
        let hash = AuthService::hash_password_for_storage(PASSWORD).expect("hash should build");
        user_repo
            .update_password(&user_id, &hash)
            .await
            .expect("password should be set");

        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("namespace should be created");

        let entry_repo = SqliteEntryRepo::new(pool.clone());
        let snapshot_repo = SqliteSnapshotRepo::new(pool.clone());
        let blob_store = FsBlobStore::new(pool.clone(), storage_root.join("blobs"));
        let upload_store = FsUploadStore::new(storage_root.join("uploads"));
        let workspace = Arc::new(DefaultWorkspaceService::new(
            entry_repo.clone(),
            snapshot_repo.clone(),
            blob_store.clone(),
            upload_store,
        ));

        let auth_service = Arc::new(AuthService::new(
            user_repo.clone(),
            SqliteSessionRepo::new(pool.clone()),
            3600,
        ));
        let namespaces = NamespaceService::new(Arc::new(namespace_repo));
        let roles = RoleFilter::new(vec![Role::Admin, Role::Manager]);
        let stats = Arc::new(IngestStats::new());

        let limiter = Arc::new(LoginAttemptLimiter::new());
        let policy = RateLimitPolicy {
            enabled: true,
            window_ms: 60_000,
            max_attempts: 5,
        };

        let authenticator = Arc::new(VfilesAuthenticator::new(
            Arc::clone(&auth_service),
            roles.clone(),
            Arc::clone(&limiter),
            policy,
            Arc::clone(&stats),
        ));
        let provider = Arc::new(VfilesUserDetailProvider::new(
            Arc::new(user_repo),
            namespaces,
            roles,
        ));

        let backend = BackendDeps {
            workspace: Arc::clone(&workspace),
            entry_repo: Arc::new(entry_repo.clone()),
            snapshot_repo: Arc::new(snapshot_repo),
            blob_store: Arc::new(blob_store),
            stats,
            max_file_size_bytes: Some(1024 * 1024),
            snapshot_mode,
            flush_threshold,
        };

        let settings = FtpSettings {
            bind: "127.0.0.1:0".parse().expect("bind addr"),
            passive_ports: (0, 0),
            passive_host: None,
            greeting: "VFiles FTP test",
            idle_timeout_secs: 60,
            tls_cert: None,
            tls_key: None,
            tls_required: false,
        };

        let app = FtpApplication {
            backend,
            authenticator,
            user_detail_provider: provider,
        };

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = spawn_ftp_server(settings.clone(), app.clone(), shutdown_rx)
            .await
            .expect("ftp server should start");

        Self {
            _temp_dir: temp_dir,
            pool,
            namespace_id,
            entry_repo,
            workspace,
            _shutdown: shutdown_tx,
            handle,
        }
    }

    /// 建立并登录一个同步 FTP 连接（在 blocking 线程中使用）。
    fn client(&self) -> FtpStream {
        let mut stream = FtpStream::connect(self.handle.local_addr()).expect("client connect");
        stream
            .login(USERNAME, PASSWORD)
            .expect("login should succeed");
        stream
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
            .find_all(&self.namespace_id)
            .await
            .expect("find_all should work");
        entries.sort_by(|a, b| a.path_norm.as_str().cmp(b.path_norm.as_str()));
        entries
            .into_iter()
            .map(|entry| entry.path_norm.as_str().to_string())
            .collect()
    }

    async fn read(&self, path: &str) -> DomainResult<vfiles_app::FileContentBytes> {
        let path = NormalizedPath::new(path).expect("path");
        self.workspace
            .read_file_bytes(&self.namespace_id, &path, None)
            .await
    }
}

fn payload(size: usize, seed: u8) -> Vec<u8> {
    (0..size)
        .map(|index| seed.wrapping_add(index as u8))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uploads_downloads_and_manages_files_over_real_ftp() {
    let harness = Harness::start(SnapshotMode::PerFile, 1).await;
    let mut client = harness.client();

    // 建目录
    client.mkdir("docs").expect("mkdir should succeed");
    client.cwd("docs").expect("cwd should succeed");

    let data = payload(4096, 7);
    let mut reader = std::io::Cursor::new(data.clone());
    let written = client
        .put_file("report.bin", &mut reader)
        .expect("put should succeed");
    assert_eq!(written as usize, data.len());

    // LIST/NLST 反映新文件
    let names = client.nlst(Some(".")).expect("nlst should succeed");
    assert!(
        names.iter().any(|name| name.contains("report.bin")),
        "nlst 应包含刚上传的文件，实际: {names:?}"
    );

    // SIZE 走 metadata
    let size = client.size("report.bin").expect("size should succeed");
    assert_eq!(size, data.len());

    // 下载内容一致
    let mut downloaded = client
        .retr_as_buffer("report.bin")
        .expect("retr should succeed");
    let mut buffer = Vec::new();
    std::io::Read::read_to_end(&mut downloaded, &mut buffer).expect("read download");
    assert_eq!(buffer, data, "下载内容应与上传一致");

    assert_eq!(harness.entry_paths().await, vec!["docs", "docs/report.bin"]);
    let stored = harness.read("docs/report.bin").await.expect("stored file");
    assert_eq!(stored.bytes, data);

    // 重命名与删除
    client
        .rename("report.bin", "renamed.bin")
        .expect("rename should succeed");
    assert_eq!(
        harness.entry_paths().await,
        vec!["docs", "docs/renamed.bin"]
    );

    client.rm("renamed.bin").expect("rm should succeed");
    assert_eq!(harness.entry_paths().await, vec!["docs"]);

    client.cwd("..").expect("cwd up should succeed");
    client.rmdir("docs").expect("rmdir should succeed");
    assert!(harness.entry_paths().await.is_empty());

    client.quit().expect("quit should succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn batch_mode_commits_far_fewer_snapshots_than_files() {
    let harness = Harness::start(SnapshotMode::Batch, 25).await;
    let mut client = harness.client();

    // 60 个文件：阈值 25 → 2 次（25+25）+ 会话结束 1 次 = 3 次
    for index in 0..60 {
        let data = payload(64, index as u8);
        let mut reader = std::io::Cursor::new(data);
        client
            .put_file(format!("file-{index:03}.bin"), &mut reader)
            .expect("put should succeed");
    }

    let before_quit = harness.snapshot_count().await;
    assert!(
        before_quit <= 3,
        "阈值 25 时 60 个文件不应产生超过 3 次快照，实际 {before_quit}"
    );

    client.quit().expect("quit should succeed");
    // 等待会话收尾任务落库
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let snapshots = harness.snapshot_count().await;
    assert!(
        (2..=4).contains(&snapshots),
        "批量模式快照数应远小于文件数（60），实际 {snapshots}"
    );

    let entries = harness.entry_paths().await;
    assert_eq!(entries.len(), 60, "60 个文件都应落库");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejects_wrong_password_and_path_traversal() {
    let harness = Harness::start(SnapshotMode::PerFile, 1).await;

    let mut anonymous = FtpStream::connect(harness.handle.local_addr()).expect("connect");
    let failed = anonymous.login(USERNAME, "wrong-password");
    assert!(failed.is_err(), "错误口令必须被拒绝");

    let mut client = harness.client();
    // `..` 在命名空间根之上会被夹取，因此这里的路径解析为 "etc/passwd"
    // （命名空间内不存在），而不是真的读到宿主机的 /etc/passwd
    let traversal = client.retr_as_buffer("../../etc/passwd");
    assert!(traversal.is_err(), "路径穿越不能命中命名空间外的文件");
    // CDUP / CWD .. 是客户端常用操作，必须仍然可用
    client.cwd("..").expect("cwd .. 应可用（夹取在根）");
    client.quit().ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn enforces_max_file_size() {
    let harness = Harness::start(SnapshotMode::PerFile, 1).await;
    let mut client = harness.client();

    // 上限 1MB：上传 1MB + 1 字节应失败
    let oversized = payload(1024 * 1024 + 1, 3);
    let mut reader = std::io::Cursor::new(oversized);
    let result = client.put_file("too-big.bin", &mut reader);
    assert!(result.is_err(), "超过上限的上传必须失败，实际: {result:?}");

    assert!(
        !harness
            .entry_paths()
            .await
            .contains(&"too-big.bin".to_string()),
        "超限文件不应留下条目"
    );
    client.quit().ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn non_empty_directory_cannot_be_removed() {
    let harness = Harness::start(SnapshotMode::PerFile, 1).await;
    let mut client = harness.client();

    client.mkdir("keep").expect("mkdir");
    let mut reader = std::io::Cursor::new(payload(16, 1));
    client
        .put_file("keep/file.txt", &mut reader)
        .expect("put into directory");

    let removed = client.rmdir("keep");
    assert!(removed.is_err(), "非空目录的 RMD 必须失败");

    client.quit().ok();
}
