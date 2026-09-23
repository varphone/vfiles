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
use vfiles_app::{AuthService, DefaultWorkspaceService, NamespaceService};

use vfiles_domain::types::RegisterRequest;
use vfiles_domain::{EntryRepo, NormalizedPath};
use vfiles_infra_sqlite::{
    FsBlobStore, FsUploadStore, SqliteEntryRepo, SqliteMigrations, SqliteNamespaceRepo,
    SqlitePoolFactory, SqliteSessionRepo, SqliteSnapshotRepo, SqliteUserRepo, SqliteWebdavLockRepo,
};
use vfiles_webdav::{WebdavApplication, WebdavWriteOps};

const USERNAME: &str = "davuser";
const PASSWORD: &str = "dav-password-1234";

/// 测试写门面桩（e2e 覆盖读面 ✓ 写面 = r108' 单测已护 ✓ 桩实现空转）。
struct NoopWrite {
    deletes: Arc<std::sync::atomic::AtomicUsize>,
    entry_repo: Arc<SqliteEntryRepo>,
}

#[async_trait::async_trait]
impl WebdavWriteOps for NoopWrite {
    async fn get_stream(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _path: &NormalizedPath,
    ) -> vfiles_domain::DomainResult<
        Option<(Box<dyn vfiles_domain::ReadSeek + Send + Unpin>, String, u64)>,
    > {
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
    async fn copy_entry(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _source: &NormalizedPath,
        _destination: &NormalizedPath,
        _user_id: &vfiles_domain::UserId,
        _overwrite: bool,
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
    async fn move_entry_with_overwrite(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _from: &NormalizedPath,
        _to: &NormalizedPath,
        _uid: &vfiles_domain::types::UserId,
        _overwrite: bool,
    ) -> vfiles_domain::DomainResult<()> {
        Ok(())
    }
    async fn move_entry_with_property_changes(
        &self,
        ns: &vfiles_domain::NamespaceId,
        from: &NormalizedPath,
        to: &NormalizedPath,
        _uid: &vfiles_domain::types::UserId,
        changes: &[vfiles_domain::EntryPropertyChange],
    ) -> vfiles_domain::DomainResult<()> {
        let entry = self
            .entry_repo
            .find_by_path(ns, from)
            .await?
            .ok_or_else(|| vfiles_domain::DomainError::NotFound {
                resource: from.as_str().to_string(),
            })?;
        self.entry_repo
            .move_entries_with_property_changes(&[(entry.id, to.clone())], &entry.id, changes)
            .await
    }
    async fn delete_entry(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _path: &NormalizedPath,
        _uid: &vfiles_domain::types::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        self.deletes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    for path in ["persist.txt", "target.txt"] {
        entry_repo
            .create_entry(
                &namespace_id,
                &NormalizedPath::new(path).expect("valid fixture path"),
                vfiles_domain::types::EntryKind::File,
                &user.id,
            )
            .await
            .expect("create fixture entry");
    }
    let persist_path = NormalizedPath::new("persist.txt").expect("fixture path");
    let persist_entry = entry_repo
        .find_by_path(&namespace_id, &persist_path)
        .await
        .expect("find fixture")
        .expect("fixture exists");
    let persist_version = entry_repo
        .create_version(
            &persist_entry.id,
            None,
            None,
            0,
            Some("text/plain"),
            &user.id,
            Some("mtime fixture"),
        )
        .await
        .expect("create fixture version");
    sqlx::query("UPDATE entry_versions SET created_at = ? WHERE id = ?")
        .bind("2030-01-02T03:04:05Z")
        .bind(persist_version.id.to_string())
        .execute(&pool)
        .await
        .expect("set fixture version timestamp");
    let deletes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
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
        audit: None,
        mount_prefix: String::new(), // e2e 直连 router（不经 nest ✗ "" = 独立语义零前缀）
        namespaces: namespaces.clone(),
        entry_repo: entry_repo.clone() as Arc<dyn EntryRepo + Send + Sync>,
        verify,
        locks: Arc::new(vfiles_webdav::LockTable::new(Arc::new(
            SqliteWebdavLockRepo::new(pool.clone()),
        ))),
        write: Arc::new(NoopWrite {
            deletes: Arc::clone(&deletes),
            entry_repo: entry_repo.clone(),
        }),
    };
    let router = vfiles_webdav::router_for_e2e(app.clone());
    let restarted_router = vfiles_webdav::router_for_e2e(WebdavApplication {
        locks: Arc::new(vfiles_webdav::LockTable::new(Arc::new(
            SqliteWebdavLockRepo::new(pool.clone()),
        ))),
        ..app
    });

    // ① OPTIONS = 能力宣告（免认证 ✓ RFC 语义）
    let resp = router
        .clone()
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
    assert!(
        resp.headers()
            .get("allow")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("PROPFIND")
    );

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
        .clone()
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
        axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("multistatus"));
    assert!(body.contains("displayname"));
    let expected_mtime_timestamp = time::OffsetDateTime::parse(
        "2030-01-02T03:04:05Z",
        &time::format_description::well_known::Rfc3339,
    )
    .expect("fixture timestamp")
    .unix_timestamp() as u64;
    let expected_mtime = httpdate::fmt_http_date(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(expected_mtime_timestamp),
    );
    assert!(body.contains(&format!(
        "<D:getlastmodified>{expected_mtime}</D:getlastmodified>"
    )));

    let patched = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPPATCH")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propertyupdate xmlns:D="DAV:" xmlns:X="urn:example:x" xmlns:Y="urn:example:y">
                        <D:set><D:prop><X:displayname>extension X</X:displayname><Y:displayname>extension Y</Y:displayname></D:prop></D:set>
                    </D:propertyupdate>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patched.status(), 207);

    let namespaced_props = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propfind xmlns:D="DAV:" xmlns:X="urn:example:x" xmlns:Y="urn:example:y">
                        <D:prop><D:displayname/><X:displayname/><Y:displayname/></D:prop>
                    </D:propfind>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(namespaced_props.status(), 207);
    let namespaced_body = String::from_utf8(
        axum::body::to_bytes(namespaced_props.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(namespaced_body.contains("<D:displayname>persist.txt</D:displayname>"));
    assert!(
        namespaced_body
            .contains("<X:displayname xmlns:X=\"urn:example:x\">extension X</X:displayname>")
    );
    assert!(
        namespaced_body
            .contains("<X:displayname xmlns:X=\"urn:example:y\">extension Y</X:displayname>")
    );

    let rejected_patch = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPPATCH")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propertyupdate xmlns:D="DAV:" xmlns:X="urn:example:x">
                        <D:set><D:prop><X:rollback>must not persist</X:rollback></D:prop></D:set>
                        <D:set><D:prop><D:getetag>protected</D:getetag></D:prop></D:set>
                    </D:propertyupdate>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected_patch.status(), 207);
    let rejected_body = String::from_utf8(
        axum::body::to_bytes(rejected_patch.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(rejected_body.contains("424 Failed Dependency"));
    assert!(rejected_body.contains("403 Forbidden"));

    let rollback_check = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propfind xmlns:D="DAV:" xmlns:X="urn:example:x"><D:prop><X:rollback/></D:prop></D:propfind>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let rollback_body = String::from_utf8(
        axum::body::to_bytes(rollback_check.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(rollback_body.contains("404 Not Found"));
    assert!(!rollback_body.contains("must not persist"));

    let remove_missing = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPPATCH")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propertyupdate xmlns:D="DAV:" xmlns:X="urn:example:x"><D:remove><D:prop><X:absent/></D:prop></D:remove></D:propertyupdate>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remove_missing.status(), 207);
    let remove_body = String::from_utf8(
        axum::body::to_bytes(remove_missing.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(remove_body.contains("200 OK"));
    assert!(!remove_body.contains("403 Forbidden"));

    let mixed_rename = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPPATCH")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propertyupdate xmlns:D="DAV:" xmlns:X="urn:example:x">
                        <D:set><D:prop><X:rejected>must not persist</X:rejected></D:prop></D:set>
                        <D:set><D:prop><D:displayname>renamed.txt</D:displayname></D:prop></D:set>
                    </D:propertyupdate>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mixed_rename.status(), 207);
    let mixed_rename_body = String::from_utf8(
        axum::body::to_bytes(mixed_rename.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(mixed_rename_body.contains("200 OK"));

    let mixed_rename_check = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propfind xmlns:D="DAV:" xmlns:X="urn:example:x"><D:prop><D:displayname/><X:rejected/></D:prop></D:propfind>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let mixed_check_body = String::from_utf8(
        axum::body::to_bytes(mixed_rename_check.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(mixed_check_body.contains("<D:displayname>renamed.txt</D:displayname>"));
    assert!(mixed_check_body.contains("must not persist"));

    // A separate WebDAV application instance sees the same SQLite-backed lock.
    let lock_body = r#"<?xml version="1.0"?>
        <D:lockinfo xmlns:D="DAV:">
          <D:lockscope><D:exclusive/></D:lockscope>
          <D:locktype><D:write/></D:locktype>
        </D:lockinfo>"#;
    let lock = restarted_router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("LOCK")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .header("timeout", "Second-600")
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(lock_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(lock.status(), 200);
    let lock_token = lock
        .headers()
        .get("lock-token")
        .expect("LOCK should return its token")
        .to_str()
        .expect("lock token should be ASCII")
        .to_string();

    let blocked_write = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .body(axum::body::Body::from("must stay locked"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_write.status(), 423);

    let wrong_token = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if", "(<opaquelocktoken:wrong>)")
                .body(axum::body::Body::from("must stay locked"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_token.status(), 412);

    let tagged_list_token = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if", format!("</persist.txt> ({lock_token})"))
                .body(axum::body::Body::from("authorized by resource-tagged list"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tagged_list_token.status(), 201);

    let alternative_list_token = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if", format!("(<opaquelocktoken:other>) ({lock_token})"))
                .body(axum::body::Body::from("authorized by alternative list"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(alternative_list_token.status(), 201);

    let rejected_move = router
        .oneshot(
            axum::http::Request::builder()
                .method("MOVE")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("destination", "/target.txt")
                .header(
                    "if",
                    "</persist.txt> (<opaquelocktoken:wrong>) </target.txt> (Not <opaquelocktoken:wrong>)",
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected_move.status(), 412);
    assert_eq!(
        deletes.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "MOVE must reject a stale source condition before deleting its destination"
    );
    let _ = user;
}
