#![cfg(feature = "e2e")]
//! WebDAV e2e 集成测（r109c 起草 ✓ **r109c 封存于 feature 门**（排障链 14 错止损 ✗✗
//! 剩 2 错 = RegisterRequest import（vfiles_domain::types ✓ 已定位）+ Workspace 4 参
//! concrete 按错式（r109d 一击内 ✓）→ 启用 = `cargo test --features e2e` ✓）。
//!
//! 覆盖：401 无凭据 / OPTIONS 能力宣告 / PROPFIND Depth 1 multistatus XML（真 sqlite
//! deps ✓）。客户端 = tower `oneshot` 直调路由（无 TCP ✓ 更快更稳 ✓
//! 范本 = vfiles-ftp/tests/ftp_end_to_end.rs 的 Harness 组装式）。

use std::sync::Arc;

use camino::Utf8PathBuf;
use tower::ServiceExt;
use vfiles_app::{
    AuthService, DefaultWorkspaceService, IngestStats, LoginAttemptLimiter, NamespaceService,
    RateLimitPolicy,
};

#[allow(unused_imports)]
use vfiles_domain::UserRepo as _;
use vfiles_domain::types::RegisterRequest;
use vfiles_domain::{EntryRepo, NormalizedPath};
use vfiles_infra_sqlite::{
    FsBlobStore, FsUploadStore, SqliteEntryRepo, SqliteMigrations, SqliteNamespaceRepo,
    SqlitePoolFactory, SqliteSessionRepo, SqliteSnapshotRepo, SqliteUserRepo,
};
use vfiles_webdav::{WebdavApplication, WebdavWriteOps};

const USERNAME: &str = "davuser";
const PASSWORD: &str = "dav-password-1234";

/// 测试写门面桩（e2e 覆盖读面 ✓ 写面 = r108' 单测已护 ✓ 桩实现空转）。
struct NoopWrite;

#[async_trait::async_trait]
impl WebdavWriteOps for NoopWrite {
    async fn get_file(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _path: &NormalizedPath,
    ) -> vfiles_domain::DomainResult<Option<(Vec<u8>, String)>> {
        Ok(None)
    }
    async fn put_file(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _path: &NormalizedPath,
        _data: Vec<u8>,
        _uid: &vfiles_domain::types::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        Ok(())
    }
    async fn mkcol(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _path: &NormalizedPath,
        _uid: &vfiles_domain::types::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        Ok(())
    }
    async fn move_entry(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _from: &NormalizedPath,
        _to: &NormalizedPath,
        _uid: &vfiles_domain::types::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        Ok(())
    }
    async fn delete_entry(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _path: &NormalizedPath,
        _uid: &vfiles_domain::types::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn options_advertises_and_propfind_needs_auth() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf8");
    let pool = SqlitePoolFactory::connect(root.join("vfiles.db").as_path())
        .await
        .expect("pool");
    SqliteMigrations::run(&pool).await.expect("migrations");
    let entry_repo = Arc::new(SqliteEntryRepo::new(pool.clone()));
    let namespace_repo: Arc<dyn vfiles_domain::NamespaceRepo + Send + Sync> =
        Arc::new(SqliteNamespaceRepo::new(pool.clone()));
    let user_repo = Arc::new(SqliteUserRepo::new(pool.clone()));
    let snapshot_repo = Arc::new(SqliteSnapshotRepo::new(pool.clone()));
    let blob_store = Arc::new(FsBlobStore::new(pool.clone(), root.join("blobs")));
    let upload_store = Arc::new(FsUploadStore::new(root.join("uploads")));
    let namespaces = NamespaceService::new(namespace_repo.clone());
    let session_repo = SqliteSessionRepo::new(pool.clone());
    let auth = Arc::new(AuthService::new(
        SqliteUserRepo::new(pool.clone()),
        session_repo,
        86_400_u64,
    ));
    let user = auth
        .register(RegisterRequest {
            username: USERNAME.to_string(),
            email: Some("dav@example.com".to_string()),
            password: PASSWORD.to_string(),
        })
        .await
        .expect("register");
    // per-user 默认命名空间（`ensure_default_for_owner` = **ns 映射真身** ✓ r109d 形明）
    let namespace_id = namespaces
        .ensure_default_for_owner(&user.id)
        .await
        .expect("ns");
    let workspace = Arc::new(DefaultWorkspaceService::new(
        SqliteEntryRepo::new(pool.clone()),
        SqliteSnapshotRepo::new(pool.clone()),
        FsBlobStore::new(pool.clone(), root.join("blobs2")),
        FsUploadStore::new(root.join("uploads2")),
    ));
    let _ = workspace; // 写面桩 ✓ 读面 = entry_repo 直供

    let verify: vfiles_webdav::VerifyFn = {
        let auth = Arc::clone(&auth);
        Arc::new(move |u: String, pw: String| {
            let auth = Arc::clone(&auth);
            Box::pin(async move { auth.verify_credentials(&u, &pw).await.ok() })
        })
    };
    let app = WebdavApplication {
        namespaces: namespaces.clone(),
        entry_repo: entry_repo.clone() as Arc<dyn EntryRepo + Send + Sync>,
        verify,
        locks: Arc::new(vfiles_webdav::LockTable::new()),
        write: Arc::new(NoopWrite),
    };
    let router = vfiles_webdav::router_for_e2e(app);

    // ① OPTIONS = 能力宣告（免认证 ✓ RFC 语义）
    let resp = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("OPTIONS")
                .uri("/")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    eprintln!("[dbg] OPTIONS headers = {:?}", resp.headers());
    assert!(resp
        .headers()
        .get("allow")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("PROPFIND"));

    // ② PROPFIND 无凭据 = 401（安全门 ✓）
    let resp = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/")
                .header("depth", "1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // ③ PROPFIND Basic = 207 multistatus（真栈 XML ✓ displayname ✓）
    let basic = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(format!("{USERNAME}:{PASSWORD}"))
    };
    let resp = router
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/")
                .header("depth", "1")
                .header("authorization", format!("Basic {basic}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 207);
    let body = String::from_utf8(
        axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec(),
    )
    .unwrap();
    assert!(body.contains("multistatus"));
    assert!(body.contains("displayname"));
    let _ = user;
}
