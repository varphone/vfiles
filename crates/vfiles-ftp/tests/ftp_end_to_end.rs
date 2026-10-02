//! 端到端集成测试：真实 TCP + 真实 FTP 客户端（suppaftp）。
//!
//! 覆盖登录、目录创建、批量上传、下载、重命名、删除，以及
//! 「批量上传只提交一次快照」这一批量导入的关键行为。

use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};

use camino::Utf8PathBuf;
use sqlx::{QueryBuilder, Sqlite};
use suppaftp::{
    FtpStream, RustlsConnector, RustlsFtpStream, Status,
    rustls::{
        ClientConfig, RootCertStore,
        pki_types::{CertificateDer, pem::PemObject},
    },
};
use tokio::sync::watch;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{TcpListener, TcpStream},
};
use unftp_core::storage::{ErrorKind, StorageBackend};
use vfiles_app::{
    AuthService, DefaultWorkspaceService, IngestStats, LoginAttemptLimiter, NamespaceService,
    RateLimitPolicy, SnapshotMode,
};
use vfiles_domain::{DomainResult, EntryRepo, NamespaceRepo, NormalizedPath, Role, UserRepo};
use vfiles_ftp::{
    BackendDeps, FtpApplication, FtpSettings, RoleFilter, VfilesAuthenticator, VfilesFtpUser,
    VfilesStorageBackend, VfilesUserDetailProvider, spawn_ftp_server,
};
use vfiles_infra_sqlite::{
    FsBlobStore, FsUploadStore, SqliteEntryRepo, SqliteMigrations, SqliteNamespaceRepo,
    SqlitePoolFactory, SqliteSessionRepo, SqliteSnapshotRepo, SqliteUserRepo, SqliteWebdavLockRepo,
};

const USERNAME: &str = "ftpuser";
const PASSWORD: &str = "ftp-password-123456";
const TEST_PASSIVE_PORT_BLOCK_SIZE: u16 = 8;
// Bound simultaneously active FTP/FTPS fixtures; unrestricted parallel runs intermittently failed
// during initial control-channel handshakes.
static TEST_HARNESS_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
static NEXT_TEST_PASSIVE_PORT: AtomicU16 = AtomicU16::new(50_000);

fn next_test_passive_ports() -> (u16, u16) {
    let start = NEXT_TEST_PASSIVE_PORT.fetch_add(TEST_PASSIVE_PORT_BLOCK_SIZE, Ordering::Relaxed);
    (start, start + TEST_PASSIVE_PORT_BLOCK_SIZE - 1)
}

struct Harness {
    _temp_dir: tempfile::TempDir,
    pool: sqlx::SqlitePool,
    namespace_id: vfiles_domain::NamespaceId,
    user_id: vfiles_domain::UserId,
    user_repo: SqliteUserRepo,
    entry_repo: SqliteEntryRepo,
    workspace: Arc<DefaultWorkspaceService>,
    backend: BackendDeps,
    _shutdown: watch::Sender<bool>,
    handle: vfiles_ftp::FtpServerHandle,
    _test_slot: tokio::sync::SemaphorePermit<'static>,
}

impl Harness {
    async fn start(snapshot_mode: SnapshotMode, flush_threshold: usize) -> Self {
        Self::start_with_limits(snapshot_mode, flush_threshold, 8, 60).await
    }

    async fn start_with_idle_timeout(
        snapshot_mode: SnapshotMode,
        flush_threshold: usize,
        idle_timeout_secs: u64,
    ) -> Self {
        Self::start_with_limits(snapshot_mode, flush_threshold, 8, idle_timeout_secs).await
    }

    async fn start_with_max_connections(
        snapshot_mode: SnapshotMode,
        flush_threshold: usize,
        max_connections: u32,
    ) -> Self {
        Self::start_with_limits(snapshot_mode, flush_threshold, max_connections, 60).await
    }

    async fn start_with_limits(
        snapshot_mode: SnapshotMode,
        flush_threshold: usize,
        max_connections: u32,
        idle_timeout_secs: u64,
    ) -> Self {
        let test_slot = TEST_HARNESS_SLOTS
            .acquire()
            .await
            .expect("test harness limiter should remain open");
        // The certificate guard intentionally rejects non-sticky shared-writable ancestors;
        // this environment's configured temp root is such a directory, so use sticky /tmp directly.
        let temp_dir = tempfile::tempdir_in("/tmp").expect("private tempdir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(temp_dir.path(), std::fs::Permissions::from_mode(0o700))
                .expect("private test directory");
        }
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
        let backend_user_repo = Arc::new(user_repo.clone());
        let provider = Arc::new(VfilesUserDetailProvider::new(
            Arc::new(user_repo.clone()),
            namespaces,
            roles,
        ));

        let backend = BackendDeps {
            workspace: Arc::clone(&workspace),
            entry_repo: Arc::new(entry_repo.clone()),
            snapshot_repo: Arc::new(snapshot_repo),
            blob_store: Arc::new(blob_store),
            user_repo: backend_user_repo,
            lock_repo: Arc::new(SqliteWebdavLockRepo::new(pool.clone())),
            stats,
            max_file_size_bytes: Some(1024 * 1024),
            snapshot_mode,
            flush_threshold,
        };

        let cert_path = temp_dir.path().join("ftp-tls/ftp-cert.pem");
        let key_path = temp_dir.path().join("ftp-tls/ftp-key.pem");
        let tls_subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];

        let settings = FtpSettings {
            bind: "127.0.0.1:0".parse().expect("bind addr"),
            passive_ports: next_test_passive_ports(),
            max_connections,
            passive_host: None,
            greeting: "VFiles FTP test",
            idle_timeout_secs,
            tls_cert: Some(cert_path.to_string_lossy().into_owned()),
            tls_key: Some(key_path.to_string_lossy().into_owned()),
            tls_self_signed: true,
            tls_subject_alt_names,
            tls_required: true,
        };

        let app = FtpApplication {
            backend: backend.clone(),
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
            user_id,
            user_repo,
            entry_repo,
            workspace,
            backend,
            _shutdown: shutdown_tx,
            handle,
            _test_slot: test_slot,
        }
    }

    /// 建立一个已升级到 FTPS 的同步连接（在 blocking 线程中使用）。
    fn secure_client(&self) -> RustlsFtpStream {
        let certificate_path = self._temp_dir.path().join("ftp-tls/ftp-cert.pem");
        let certificate_pem = std::fs::read(certificate_path).expect("test certificate");
        let certificate = CertificateDer::pem_slice_iter(&certificate_pem)
            .next()
            .expect("PEM certificate should be present")
            .expect("PEM certificate should parse");
        let mut roots = RootCertStore::empty();
        roots
            .add(certificate)
            .expect("test certificate should be trusted");
        let tls_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        RustlsFtpStream::connect(self.handle.local_addr())
            .expect("client connect")
            .into_secure(RustlsConnector::from(Arc::new(tls_config)), "localhost")
            .expect("control channel should use FTPS")
    }

    /// 建立并登录一个同步 FTPS 连接。
    fn client(&self) -> RustlsFtpStream {
        let mut stream = self.secure_client();
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

    async fn disable_user(&self) {
        self.user_repo
            .disable_user(&self.user_id)
            .await
            .expect("user should be disabled");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejects_new_control_connections_at_the_configured_limit() {
    let harness = Harness::start_with_max_connections(SnapshotMode::Off, 1, 1).await;
    let address = harness.handle.local_addr();

    let first = TcpStream::connect(address).await.expect("first connection");
    let mut first_reader = BufReader::new(first);
    let mut first_greeting = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        first_reader.read_line(&mut first_greeting),
    )
    .await
    .expect("first greeting timeout")
    .expect("first greeting read");
    assert!(first_greeting.starts_with("220 "));

    let second = TcpStream::connect(address)
        .await
        .expect("second TCP connect");
    let mut second_reader = BufReader::new(second);
    let mut second_greeting = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        second_reader.read_line(&mut second_greeting),
    )
    .await
    .expect("rejected connection close timeout")
    .expect("rejected connection read");
    assert!(
        second_greeting.is_empty(),
        "会话数达到上限后服务端应立即关闭新连接"
    );

    drop(first_reader);
    let mut admitted = false;
    for _ in 0..20 {
        let candidate = TcpStream::connect(address).await.expect("retry connect");
        let mut reader = BufReader::new(candidate);
        let mut greeting = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut greeting),
        )
        .await
        .expect("retry greeting timeout")
        .expect("retry greeting read");
        if greeting.starts_with("220 ") {
            admitted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(admitted, "释放会话后应接纳新连接");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unauthenticated_noop_cannot_extend_the_absolute_login_deadline() {
    let harness = Harness::start_with_idle_timeout(SnapshotMode::Off, 1, 2).await;
    let mut client = harness.secure_client();

    for _ in 0..4 {
        client
            .noop()
            .expect("unauthenticated NOOP should work before the login deadline");
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(1_300)).await;

    let result = tokio::task::spawn_blocking(move || client.noop())
        .await
        .expect("FTP client task should finish");
    assert!(
        result.is_err(),
        "repeated NOOP must not keep an unauthenticated control session alive"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_epsv_commands_do_not_exhaust_the_passive_port_range() {
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let mut client = harness.client();

    for index in 0..120 {
        client
            .custom_command("EPSV", &[Status::ExtendedPassiveMode])
            .unwrap_or_else(|error| {
                panic!("EPSV request {index} should release the previous listener: {error}")
            });
    }

    client.quit().expect("quit should succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quit_closes_a_pending_passive_listener() {
    let harness = Harness::start_with_max_connections(SnapshotMode::Off, 1, 1).await;
    let mut client = harness.client();
    let response = client
        .custom_command("EPSV", &[Status::ExtendedPassiveMode])
        .expect("EPSV should allocate a passive listener");
    let passive_port = response
        .as_string()
        .expect("EPSV response should be UTF-8")
        .split("|||")
        .nth(1)
        .and_then(|port| port.split('|').next())
        .expect("EPSV response should contain its port")
        .parse::<u16>()
        .expect("EPSV port should be a number");
    let passive_address = std::net::SocketAddr::new(harness.handle.local_addr().ip(), passive_port);

    client.quit().expect("quit should succeed");
    drop(client);

    let mut listener_closed = false;
    for _ in 0..40 {
        match TcpListener::bind(passive_address).await {
            Ok(listener) => {
                listener_closed = true;
                drop(listener);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            Err(error) => panic!("could not check passive listener cleanup: {error}"),
        }
    }

    assert!(
        listener_closed,
        "QUIT must release a passive port before its 15-second accept timeout"
    );

    let address = harness.handle.local_addr();
    let mut replacement_accepted = false;
    for _ in 0..20 {
        let candidate = TcpStream::connect(address)
            .await
            .expect("replacement TCP connection should open");
        let mut reader = BufReader::new(candidate);
        let mut greeting = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut greeting),
        )
        .await
        .expect("replacement greeting timeout")
        .expect("replacement greeting read");
        if greeting.starts_with("220 ") {
            replacement_accepted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        replacement_accepted,
        "QUIT cleanup must release the session slot after closing its passive listener"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ftps_rejects_control_and_data_channel_downgrades() {
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let mut client = harness.client();

    assert!(
        client.custom_command("CCC", &[Status::CommandOk]).is_err(),
        "FTPS must reject clearing the protected control channel"
    );
    assert!(
        client
            .custom_command("PROT C", &[Status::CommandOk])
            .is_err(),
        "FTPS must reject clearing data-channel protection"
    );
    assert!(
        client.nlst(Some(".")).is_ok(),
        "the session should retain protected data transfers after downgrade attempts"
    );

    client.quit().expect("quit should succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ftps_rejects_repeated_control_channel_upgrade() {
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let mut client = harness.client();

    assert!(
        client
            .custom_command("AUTH TLS", &[Status::AuthOk])
            .is_err(),
        "an already encrypted control channel must reject a second TLS upgrade"
    );
    client
        .noop()
        .expect("the existing FTPS control channel should remain usable");
    client.quit().expect("quit should succeed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabled_user_stops_receiving_a_directory_listing_in_progress() {
    const ENTRY_COUNT: usize = 3_500;
    const NAME_PADDING: usize = 850;
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let rows: Vec<(String, String)> = (0..ENTRY_COUNT)
        .map(|index| {
            (
                format!("00000000-0000-4000-8000-{index:012x}"),
                format!("slow-list-{index:05}-{}", "x".repeat(NAME_PADDING)),
            )
        })
        .collect();
    let mut transaction = harness.pool.begin().await.expect("transaction");
    for chunk in rows.chunks(1_000) {
        let mut query =
            QueryBuilder::<Sqlite>::new("INSERT INTO entries (id, namespace_id, path, kind) ");
        query.push_values(chunk, |mut builder, (id, path)| {
            builder
                .push_bind(id)
                .push_bind(harness.namespace_id.to_string())
                .push_bind(path)
                .push_bind("directory");
        });
        query
            .build()
            .execute(&mut *transaction)
            .await
            .expect("directory fixtures should be inserted");
    }
    transaction.commit().await.expect("fixture transaction");

    let record = harness
        .user_repo
        .find_by_id(&harness.user_id)
        .await
        .expect("FTP user");
    let user = VfilesFtpUser {
        id: record.id,
        username: record.username.to_string(),
        role: record.role,
        namespace_id: harness.namespace_id,
        account_updated_at: record.updated_at,
        password_changed_at: record.password_changed_at,
        anonymous: false,
    };
    let backend = VfilesStorageBackend::new(harness.backend.clone());
    backend
        .list(&user, ".")
        .await
        .expect("slow-list fixture should remain under the listing limit");

    let mut client = harness.client();
    let (listing_started_tx, listing_started_rx) = tokio::sync::oneshot::channel();
    let (resume_reading_tx, resume_reading_rx) = std::sync::mpsc::channel();
    let transfer = tokio::task::spawn_blocking(move || {
        let (_, mut data_stream) = client
            .custom_data_command("NLST .", &[Status::AboutToSend])
            .expect("NLST should start");
        data_stream
            .get_ref()
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("data read timeout should be set");
        client
            .get_ref()
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .expect("control read timeout should be set");
        let mut data = vec![0; 1_024];
        std::io::Read::read_exact(&mut data_stream, &mut data)
            .expect("listing should write data before account revocation");
        listing_started_tx
            .send(())
            .expect("test should still be waiting for NLST");
        resume_reading_rx
            .recv()
            .expect("test should resume data reading");

        let data_result = std::io::Read::read_to_end(&mut data_stream, &mut data);
        let data_bytes = data.len();
        let control_result = client.close_data_connection(data_stream);
        (data_result, data_bytes, control_result)
    });

    listing_started_rx
        .await
        .expect("NLST should reach the data phase");
    harness.disable_user().await;
    tokio::time::sleep(std::time::Duration::from_millis(5_200)).await;
    resume_reading_tx
        .send(())
        .expect("data reader should still be alive");

    let (data_result, data_bytes, control_result) =
        tokio::time::timeout(std::time::Duration::from_secs(10), transfer)
            .await
            .expect("revoked listing should stop promptly")
            .expect("FTP client task should finish");
    let expected_bytes =
        ENTRY_COUNT * (format!("slow-list-{:05}-{}", 0, "x".repeat(NAME_PADDING)).len() + 2);
    assert!(
        data_result.is_ok(),
        "FTPS should close the listing stream cleanly after revocation: {data_result:?}"
    );
    assert!(
        (1_024..expected_bytes).contains(&data_bytes),
        "a revoked account must not receive the buffered directory listing ({data_bytes}/{expected_bytes} bytes)"
    );
    assert!(
        control_result.is_err(),
        "the control channel should report the revoked listing instead of success"
    );
}

fn payload(size: usize, seed: u8) -> Vec<u8> {
    (0..size)
        .map(|index| seed.wrapping_add(index as u8))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uploads_downloads_and_manages_files_over_ftps() {
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
async fn rejected_rest_upload_does_not_leak_its_offset_to_later_retr() {
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let mut client = harness.client();
    let original = b"0123456789".to_vec();
    let mut original_reader = std::io::Cursor::new(original.clone());
    client
        .put_file("restart.bin", &mut original_reader)
        .expect("initial upload should succeed");

    client
        .custom_command("REST 4", &[Status::RequestFilePending])
        .expect("RETR restart should be accepted");
    let suffix = client
        .retr_as_buffer("restart.bin")
        .expect("resumed RETR should succeed")
        .into_inner();
    assert_eq!(suffix, original[4..]);

    let mut replacement = std::io::Cursor::new(b"replacement".to_vec());
    client
        .custom_command("REST 4", &[Status::RequestFilePending])
        .expect("REST before upload should be accepted by the protocol layer");
    assert!(
        client.put_file("restart.bin", &mut replacement).is_err(),
        "the VFiles backend does not support resumed uploads"
    );

    let after_rejected_upload = client
        .retr_as_buffer("restart.bin")
        .expect("full RETR after the rejected upload should succeed")
        .into_inner();
    assert_eq!(
        after_rejected_upload, original,
        "a rejected STOR must clear REST state so it cannot truncate the next RETR"
    );
    client.quit().ok();
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

    let mut plaintext = FtpStream::connect(harness.handle.local_addr()).expect("plain connect");
    assert!(
        plaintext.login(USERNAME, PASSWORD).is_err(),
        "服务端必须拒绝明文登录"
    );

    let mut anonymous = harness.secure_client();
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
async fn ftp_login_rate_limit_combines_email_case_variants() {
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let identifiers = [
        "ftp@example.com",
        "FTP@EXAMPLE.COM",
        "Ftp@Example.Com",
        "ftp@example.com",
        "FTP@EXAMPLE.COM",
    ];

    for identifier in identifiers {
        let mut client = harness.secure_client();
        assert!(
            client.login(identifier, "wrong-password").is_err(),
            "invalid credentials must fail for {identifier}"
        );
    }

    let mut client = harness.secure_client();
    assert!(
        client.login("ftp@example.com", PASSWORD).is_err(),
        "email casing must not create fresh FTP failure counters"
    );
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rmd_flushes_pending_uploads_before_checking_directory_contents() {
    let harness = Harness::start(SnapshotMode::Batch, 100).await;
    let mut client = harness.client();

    client
        .mkdir("batch-dir")
        .expect("directory should be created");
    let snapshots_before_upload = harness.snapshot_count().await;
    let data = payload(16, 0x5a);
    let mut reader = std::io::Cursor::new(data.clone());
    client
        .put_file("batch-dir/pending.bin", &mut reader)
        .expect("upload should remain pending below the batch threshold");

    assert_eq!(
        harness.snapshot_count().await,
        snapshots_before_upload,
        "the upload should remain in the batch until an operation flushes it"
    );
    assert_eq!(
        harness.entry_paths().await,
        vec!["batch-dir", "batch-dir/pending.bin"],
        "the uploaded file should already exist in the live tree"
    );
    let snapshots_before_rmd = harness.snapshot_count().await;
    assert!(
        client.rmdir("batch-dir").is_err(),
        "RMD must see the pending upload and reject a non-empty directory"
    );
    assert!(
        harness.snapshot_count().await > snapshots_before_rmd,
        "RMD should flush the pending snapshot batch before checking the directory"
    );
    assert_eq!(
        harness.entry_paths().await,
        vec!["batch-dir", "batch-dir/pending.bin"],
        "flushing before RMD must preserve both the directory and pending file"
    );
    assert_eq!(
        harness
            .read("batch-dir/pending.bin")
            .await
            .expect("pending file should be committed")
            .bytes,
        data
    );

    client.quit().ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabled_user_cannot_keep_control_session_alive_with_noop() {
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let mut client = harness.client();
    client.noop().expect("initial NOOP should succeed");

    harness.disable_user().await;
    tokio::time::sleep(std::time::Duration::from_millis(5_100)).await;

    let result = tokio::task::spawn_blocking(move || client.noop())
        .await
        .expect("FTP client task should finish");
    assert!(result.is_err(), "revoked session must be closed on NOOP");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabled_user_cannot_bypass_revalidation_with_invalid_commands() {
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let mut client = harness.client();
    client.noop().expect("initial NOOP should succeed");

    harness.disable_user().await;
    tokio::time::sleep(std::time::Duration::from_millis(5_100)).await;

    let result = tokio::task::spawn_blocking(move || {
        client.custom_command("NOOP INVALID", &[Status::BadArguments])
    })
    .await
    .expect("FTP client task should finish");
    assert!(
        result.is_err(),
        "revoked session must be closed instead of accepting an invalid command"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabled_user_cannot_bypass_revalidation_with_ftps_policy_commands() {
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let mut ccc_client = harness.client();
    let mut prot_client = harness.client();
    ccc_client
        .noop()
        .expect("initial CCC client NOOP should succeed");
    prot_client
        .noop()
        .expect("initial PROT client NOOP should succeed");

    harness.disable_user().await;
    tokio::time::sleep(std::time::Duration::from_millis(5_100)).await;

    let (ccc_result, prot_result) = tokio::task::spawn_blocking(move || {
        (
            ccc_client.custom_command("CCC", &[Status::Unknown]),
            prot_client.custom_command("PROT C", &[Status::Unknown]),
        )
    })
    .await
    .expect("FTP client task should finish");
    assert!(
        ccc_result.is_err(),
        "revoked session must close before FTPS control-channel policy replies"
    );
    assert!(
        prot_result.is_err(),
        "revoked session must close before FTPS data-channel policy replies"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ftp_listing_rejects_directories_over_its_bounded_entry_limit() {
    const CHILD_COUNT: usize = 20_001;
    let harness = Harness::start(SnapshotMode::Off, 1).await;

    let rows: Vec<(String, String)> = (0..CHILD_COUNT)
        .map(|index| {
            (
                format!("00000000-0000-4000-8000-{index:012x}"),
                format!("ftp-list-limit-{index:05}"),
            )
        })
        .collect();
    let mut transaction = harness.pool.begin().await.expect("transaction");
    for chunk in rows.chunks(1_000) {
        let mut query =
            QueryBuilder::<Sqlite>::new("INSERT INTO entries (id, namespace_id, path, kind) ");
        query.push_values(chunk, |mut builder, (id, path)| {
            builder
                .push_bind(id)
                .push_bind(harness.namespace_id.to_string())
                .push_bind(path)
                .push_bind("directory");
        });
        query
            .build()
            .execute(&mut *transaction)
            .await
            .expect("directory fixtures should be inserted");
    }
    transaction.commit().await.expect("fixture transaction");

    let record = harness
        .user_repo
        .find_by_id(&harness.user_id)
        .await
        .expect("FTP user");
    let user = VfilesFtpUser {
        id: record.id,
        username: record.username.to_string(),
        role: record.role,
        namespace_id: harness.namespace_id,
        account_updated_at: record.updated_at,
        password_changed_at: record.password_changed_at,
        anonymous: false,
    };
    let backend = VfilesStorageBackend::new(harness.backend.clone());

    let error = match backend.list(&user, ".").await {
        Ok(_) => panic!("FTP must refuse the oversized directory listing"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ErrorKind::LocalError);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ftp_rename_rejects_subtrees_over_its_bounded_entry_limit() {
    const DESCENDANT_COUNT: usize = 20_001;
    let harness = Harness::start(SnapshotMode::Off, 1).await;
    let mut rows = Vec::with_capacity(DESCENDANT_COUNT + 1);
    rows.push((
        "ffffffff-ffff-4fff-bfff-ffffffffffff".to_string(),
        "large".to_string(),
    ));
    rows.extend((0..DESCENDANT_COUNT).map(|index| {
        (
            format!("00000000-0000-4000-8000-{index:012x}"),
            format!("large/item-{index:05}"),
        )
    }));

    let mut transaction = harness.pool.begin().await.expect("transaction");
    for chunk in rows.chunks(1_000) {
        let mut query =
            QueryBuilder::<Sqlite>::new("INSERT INTO entries (id, namespace_id, path, kind) ");
        query.push_values(chunk, |mut builder, (id, path)| {
            builder
                .push_bind(id)
                .push_bind(harness.namespace_id.to_string())
                .push_bind(path)
                .push_bind("directory");
        });
        query
            .build()
            .execute(&mut *transaction)
            .await
            .expect("subtree fixtures should be inserted");
    }
    transaction.commit().await.expect("fixture transaction");

    let mut client = harness.client();
    assert!(
        client.rename("large", "renamed").is_err(),
        "RNTO must refuse a subtree above its entry budget"
    );
    client.quit().ok();

    let original_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entries WHERE namespace_id = ? AND (path = 'large' OR path LIKE 'large/%')",
    )
    .bind(harness.namespace_id.to_string())
    .fetch_one(&harness.pool)
    .await
    .expect("original subtree count");
    let renamed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entries WHERE namespace_id = ? AND (path = 'renamed' OR path LIKE 'renamed/%')",
    )
    .bind(harness.namespace_id.to_string())
    .fetch_one(&harness.pool)
    .await
    .expect("renamed subtree count");
    assert_eq!(original_count, (DESCENDANT_COUNT + 1) as i64);
    assert_eq!(renamed_count, 0);
}
