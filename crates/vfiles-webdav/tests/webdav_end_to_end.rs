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
    put_bodies: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
}

#[async_trait::async_trait]
impl WebdavWriteOps for NoopWrite {
    async fn get_stream(
        &self,
        ns: &vfiles_domain::types::NamespaceId,
        path: &NormalizedPath,
    ) -> vfiles_domain::DomainResult<
        Option<(Box<dyn vfiles_domain::ReadSeek + Send + Unpin>, String, u64)>,
    > {
        if self.entry_repo.find_by_path(ns, path).await?.is_none() {
            return Ok(None);
        }
        let bytes = b"webdav range fixture".to_vec();
        let size = bytes.len() as u64;
        Ok(Some((
            Box::new(tokio::io::BufReader::new(std::io::Cursor::new(bytes))),
            "text/plain".to_string(),
            size,
        )))
    }
    async fn put_file(
        &self,
        _ns: &vfiles_domain::types::NamespaceId,
        _path: &NormalizedPath,
        mut reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        _uid: &vfiles_domain::types::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        use tokio::io::AsyncReadExt;
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await.map_err(|error| {
            vfiles_domain::DomainError::Internal {
                message: format!("failed to read test PUT stream: {error}"),
            }
        })?;
        self.put_bodies.lock().unwrap().push(data);
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
    for path in ["persist.txt", "target.txt", "space #?汉.txt"] {
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
    entry_repo
        .create_entry(
            &namespace_id,
            &NormalizedPath::new("locked-dir").expect("directory path"),
            vfiles_domain::types::EntryKind::Directory,
            &user.id,
        )
        .await
        .expect("locked directory should be created");
    entry_repo
        .create_entry(
            &namespace_id,
            &NormalizedPath::new("locked-dir/child.txt").expect("child path"),
            vfiles_domain::types::EntryKind::File,
            &user.id,
        )
        .await
        .expect("locked child should be created");
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
    let put_bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
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
            put_bodies: Arc::clone(&put_bodies),
        }),
        max_file_size_bytes: 64,
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
    let remote_destination = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("MOVE")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("host", "dav.example")
                .header("destination", "http://attacker.example/target.txt")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remote_destination.status(), 400);
    assert!(
        entry_repo
            .find_by_path(
                &namespace_id,
                &NormalizedPath::new("persist.txt").expect("fixture path"),
            )
            .await
            .expect("read source entry")
            .is_some()
    );

    let encoded_local_destination = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("MOVE")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("host", "dav.example")
                .header("destination", "http://DAV.example/target%20space.txt")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(encoded_local_destination.status(), 201);

    let default_depth = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/")
                .header("authorization", format!("Basic {basic}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(default_depth.status(), 403);
    let default_depth_error = String::from_utf8(
        axum::body::to_bytes(default_depth.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(default_depth_error.contains("propfind-finite-depth"));

    let invalid_depth = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "2")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_depth.status(), 400);

    let repeated_depth = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .header("depth", "1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeated_depth.status(), 400);

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
    assert!(body.contains("/space%20%23%3F%E6%B1%89.txt"));

    let malformed_if = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if", "not-a-valid-if-condition")
                .body(axum::body::Body::from("must not be written"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed_if.status(), 400);

    let repeated_if = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if", "([\"matching-shape\"])")
                .header("if", "([\"second-field\"])")
                .body(axum::body::Body::from("must not be written"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeated_if.status(), 400);

    let oversized_put = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/target.txt")
                .header("authorization", format!("Basic {basic}"))
                .body(axum::body::Body::from(vec![b'x'; 65]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized_put.status(), 413);
    assert!(put_bodies.lock().unwrap().is_empty());

    let broken_body = futures::stream::iter([
        Ok(axum::body::Bytes::from_static(b"partial upload")),
        Err(std::io::Error::other("simulated request body failure")),
    ]);
    let failed_stream_put = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/target.txt")
                .header("authorization", format!("Basic {basic}"))
                .body(axum::body::Body::from_stream(broken_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed_stream_put.status(), 400);
    assert!(put_bodies.lock().unwrap().is_empty());

    let oversized_xml = vec![b' '; 1024 * 1024 + 1];
    let oversized_propfind = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .body(axum::body::Body::from(oversized_xml.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized_propfind.status(), 413);

    let oversized_proppatch = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPPATCH")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .body(axum::body::Body::from(oversized_xml))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized_proppatch.status(), 413);

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
                .uri("/")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "1")
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

    let full_get = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(full_get.status(), 200);
    let etag = full_get
        .headers()
        .get("etag")
        .expect("GET should expose its current strong ETag")
        .to_str()
        .expect("ETag should be ASCII")
        .to_string();
    let last_modified = full_get
        .headers()
        .get("last-modified")
        .expect("GET should expose Last-Modified")
        .to_str()
        .expect("Last-Modified should be ASCII")
        .to_string();
    assert_eq!(
        axum::body::to_bytes(full_get.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"webdav range fixture"
    );

    let matching_if_match = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if-match", &etag)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(matching_if_match.status(), 200);

    let stale_if_match = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if-match", "\"stale\"")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_if_match.status(), 412);

    let weak_if_match = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if-match", format!("W/{etag}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(weak_if_match.status(), 412);

    let not_modified = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if-none-match", format!("W/{etag}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_modified.status(), 304);

    let date_not_modified = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if-modified-since", &last_modified)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(date_not_modified.status(), 304);

    let stale_if_unmodified_since = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if-unmodified-since", "Thu, 01 Jan 1970 00:00:00 GMT")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_if_unmodified_since.status(), 412);

    let if_match_precedes_date = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("if-match", &etag)
                .header("if-unmodified-since", "Thu, 01 Jan 1970 00:00:00 GMT")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(if_match_precedes_date.status(), 200);

    let current_if_range = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("range", "bytes=0-3")
                .header("if-range", &etag)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(current_if_range.status(), 206);
    assert_eq!(
        axum::body::to_bytes(current_if_range.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"webd"
    );

    let stale_if_range = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("range", "bytes=0-3")
                .header("if-range", "\"stale\"")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_if_range.status(), 200);
    assert_eq!(
        axum::body::to_bytes(stale_if_range.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"webdav range fixture"
    );

    let date_if_range = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("range", "bytes=0-3")
                .header("if-range", &last_modified)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(date_if_range.status(), 200);
    assert_eq!(
        axum::body::to_bytes(date_if_range.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"webdav range fixture"
    );

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

    let tagged_refresh = restarted_router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("LOCK")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("If", format!("</persist.txt> ({lock_token})"))
                .header("timeout", "Second-900")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tagged_refresh.status(), 200);
    assert_eq!(
        tagged_refresh.headers().get("lock-token"),
        Some(&axum::http::HeaderValue::from_str(&lock_token).unwrap())
    );

    let mismatched_tag_refresh = restarted_router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("LOCK")
                .uri("/persist.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("If", format!("</other.txt> ({lock_token})"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mismatched_tag_refresh.status(), 412);

    let child_lock = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("LOCK")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .header("timeout", "Second-600")
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(lock_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(child_lock.status(), 200);
    let child_lock_token = child_lock
        .headers()
        .get("lock-token")
        .expect("child LOCK should return its token")
        .to_str()
        .expect("child lock token should be ASCII")
        .to_string();

    let child_lock_props = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "1")
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propfind xmlns:D="DAV:"><D:prop><D:lockdiscovery/></D:prop></D:propfind>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(child_lock_props.status(), 207);
    let child_lock_xml = String::from_utf8(
        axum::body::to_bytes(child_lock_props.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        child_lock_xml.contains(&child_lock_token[1..child_lock_token.len() - 1]),
        "depth-one PROPFIND should expose the locked child: {child_lock_xml}"
    );

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
    assert_eq!(
        put_bodies.lock().unwrap().as_slice(),
        &[b"authorized by resource-tagged list".to_vec()]
    );

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
    assert_eq!(
        put_bodies.lock().unwrap().as_slice(),
        &[
            b"authorized by resource-tagged list".to_vec(),
            b"authorized by alternative list".to_vec(),
        ]
    );

    let rejected_move = router
        .clone()
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

    let nested_lock = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("LOCK")
                .uri("/locked-dir/child.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(lock_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(nested_lock.status(), 200);
    let nested_lock_token = nested_lock
        .headers()
        .get("lock-token")
        .expect("nested LOCK should return its token")
        .to_str()
        .expect("nested lock token should be ASCII")
        .to_string();

    let blocked_parent_delete = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/locked-dir")
                .header("authorization", format!("Basic {basic}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_parent_delete.status(), 423);
    assert_eq!(deletes.load(std::sync::atomic::Ordering::Relaxed), 0);

    let authorized_parent_delete = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/locked-dir")
                .header("authorization", format!("Basic {basic}"))
                .header(
                    "If",
                    format!("</locked-dir/child.txt> ({nested_lock_token})"),
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized_parent_delete.status(), 204);
    assert_eq!(deletes.load(std::sync::atomic::Ordering::Relaxed), 1);

    sqlx::query("DROP TABLE entry_properties")
        .execute(&pool)
        .await
        .expect("inject a property repository read failure");
    let selected_builtin_property = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "1")
                .header("content-type", "application/xml")
                .body(axum::body::Body::from(
                    r#"<D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(selected_builtin_property.status(), 207);
    let selected_builtin_xml = String::from_utf8(
        axum::body::to_bytes(selected_builtin_property.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(selected_builtin_xml.contains("<D:displayname>renamed.txt</D:displayname>"));

    let property_read_failure = router
        .oneshot(
            axum::http::Request::builder()
                .method("PROPFIND")
                .uri("/renamed.txt")
                .header("authorization", format!("Basic {basic}"))
                .header("depth", "0")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(property_read_failure.status(), 500);
    let _ = user;
}
