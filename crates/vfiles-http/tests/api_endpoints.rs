use std::{io::Cursor, sync::Arc};

use axum::{
    body::Body,
    http::{HeaderValue, Method, Request, StatusCode, header},
};
use camino::Utf8PathBuf;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::format_description::well_known::Rfc3339;
use tower::util::ServiceExt;
use vfiles_app::{
    AdminService, AuthService, HealthService, HistoryService, SearchService, SessionService,
    ShareService, UploadService, WorkspaceService,
};
use vfiles_config::ConfigLoader;
use vfiles_domain::{FeatureMatrix, NamespaceRepo, UserRepo};
use vfiles_http::{AppState, FrontendAssets, build_router, middleware::LoginAttemptLimiter};
use vfiles_infra_fs::FsStorageBootstrap;
use vfiles_infra_sqlite::{
    FsBlobStore, FsUploadStore, SqliteAdminRepo, SqliteEntryRepo, SqliteMigrations,
    SqliteNamespaceRepo, SqlitePoolFactory, SqliteSearchRepo, SqliteSessionRepo, SqliteShareRepo,
    SqliteSnapshotRepo, SqliteUserRepo,
};

struct TestApp {
    app: axum::Router<()>,
    _temp_dir: TempDir,
}

fn single_upload_multipart(
    filename: &str,
    path: &str,
    message: &str,
    bytes: &[u8],
) -> (Vec<u8>, String) {
    let boundary = "----vfiles-single-upload-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(
        format!(
            "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"path\"\r\n\r\n{path}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"message\"\r\n\r\n{message}\r\n--{boundary}--\r\n"
        )
        .as_bytes(),
    );
    (body, format!("multipart/form-data; boundary={boundary}"))
}

fn password_hash(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

impl TestApp {
    async fn new() -> Self {
        Self::new_with_settings(true, default_features(), None).await
    }

    /// 自定义单文件上限（用于验证服务端按配置上限拒绝上传）。
    async fn new_with_max_file_size_bytes(max_file_size_bytes: u64) -> Self {
        let mut features = default_features();
        features.max_file_size_bytes = max_file_size_bytes;
        Self::new_with_settings(true, features, None).await
    }

    async fn new_with_public_base_url(public_base_url: &str) -> Self {
        Self::new_with_public_base_url_and_cookie_secure(public_base_url, None).await
    }

    async fn new_with_public_base_url_and_cookie_secure(
        public_base_url: &str,
        cookie_secure_override: Option<bool>,
    ) -> Self {
        Self::new_with_settings_and_public_base_url(
            true,
            default_features(),
            None,
            Some(public_base_url.to_string()),
            cookie_secure_override,
        )
        .await
    }

    async fn new_with_allow_register(allow_register: bool) -> Self {
        Self::new_with_settings(allow_register, default_features(), None).await
    }

    async fn new_with_login_rate_limit(max_attempts: u32) -> Self {
        Self::new_with_settings_and_public_base_url_and_login_rate_limit(
            true,
            default_features(),
            None,
            None,
            None,
            Some(max_attempts),
        )
        .await
    }

    async fn new_with_features(features: FeatureMatrix) -> Self {
        Self::new_with_settings(true, features, None).await
    }

    async fn new_with_static_frontend(index_html: &str) -> Self {
        Self::new_with_settings(true, default_features(), Some(index_html.to_string())).await
    }

    async fn new_with_settings(
        allow_register: bool,
        features: FeatureMatrix,
        prebuilt_frontend: Option<String>,
    ) -> Self {
        Self::new_with_settings_and_public_base_url(
            allow_register,
            features,
            prebuilt_frontend,
            None,
            None,
        )
        .await
    }

    async fn new_with_settings_and_public_base_url(
        allow_register: bool,
        features: FeatureMatrix,
        prebuilt_frontend: Option<String>,
        public_base_url_override: Option<String>,
        cookie_secure_override: Option<bool>,
    ) -> Self {
        Self::new_with_settings_and_public_base_url_and_login_rate_limit(
            allow_register,
            features,
            prebuilt_frontend,
            public_base_url_override,
            cookie_secure_override,
            None,
        )
        .await
    }

    async fn new_with_settings_and_public_base_url_and_login_rate_limit(
        allow_register: bool,
        features: FeatureMatrix,
        prebuilt_frontend: Option<String>,
        public_base_url_override: Option<String>,
        cookie_secure_override: Option<bool>,
        login_rate_limit_max_attempts: Option<u32>,
    ) -> Self {
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        let storage_root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .expect("tempdir path should be valid utf-8");
        let database_path = storage_root.join("vfiles.db");
        let frontend_dist = prebuilt_frontend.map(|index_html| {
            let frontend_dist = storage_root.join("frontend-dist");
            std::fs::create_dir_all(frontend_dist.join("assets"))
                .expect("frontend assets dir should be created");
            std::fs::write(frontend_dist.join("index.html"), index_html)
                .expect("index.html should be written");
            std::fs::write(
                frontend_dist.join("assets/app.js"),
                "console.log('vfiles');",
            )
            .expect("frontend asset should be written");
            // 构建期预压缩产物（真实构建由 scripts/precompress.mjs 生成）。
            std::fs::write(
                frontend_dist.join("assets/app.js.br"),
                "BROTLI:console.log('vfiles');",
            )
            .expect("brotli variant should be written");
            std::fs::write(
                frontend_dist.join("assets/app.js.gz"),
                "GZIP:console.log('vfiles');",
            )
            .expect("gzip variant should be written");
            frontend_dist
        });

        FsStorageBootstrap::bootstrap(&storage_root)
            .await
            .expect("storage bootstrap should succeed");

        let pool = SqlitePoolFactory::connect(database_path.as_path())
            .await
            .expect("sqlite pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should succeed");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let admin_user_id = user_repo
            .create_admin(
                "admin",
                "admin@example.com",
                &password_hash("admin-password"),
            )
            .await
            .expect("admin user should be created");

        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let default_namespace_id = namespace_repo
            .create_default(&admin_user_id, "default")
            .await
            .expect("default namespace should be created");

        let mut config = ConfigLoader::load().expect("config should load");
        config.storage.root = storage_root.clone();
        config.storage.database_path = database_path;
        config.auth.allow_register = allow_register;
        config.auth.login_rate_limit.enabled = false;
        config.http.public_base_url = public_base_url_override
            .unwrap_or_else(|| "http://example.test:4242".to_string())
            .parse()
            .expect("base url should parse");
        config.http.cookie_secure_override = cookie_secure_override;
        config.features = features;
        if let Some(max_attempts) = login_rate_limit_max_attempts {
            config.auth.login_rate_limit.enabled = true;
            config.auth.login_rate_limit.window_ms = 300_000;
            config.auth.login_rate_limit.max_attempts = max_attempts;
        }

        let session_repo = SqliteSessionRepo::new(pool.clone());
        let auth_service = AuthService::new(
            user_repo.clone(),
            session_repo.clone(),
            config.auth.session_ttl_seconds,
        );
        let admin_service =
            AdminService::new(SqliteAdminRepo::new(pool.clone()), auth_service.clone());
        let session_service =
            SessionService::new(Some(auth_service.clone()), config.features.clone());

        let entry_repo = SqliteEntryRepo::new(pool.clone());
        let snapshot_repo = SqliteSnapshotRepo::new(pool.clone());
        let blob_store = FsBlobStore::new(pool.clone(), storage_root.join("blobs"));
        let upload_store = FsUploadStore::new(storage_root.join("uploads"));
        let share_repo = SqliteShareRepo::new(pool.clone());
        let search_repo = SqliteSearchRepo::new(pool.clone(), blob_store.clone());
        let history_service = HistoryService::new(
            entry_repo.clone(),
            snapshot_repo.clone(),
            blob_store.clone(),
            user_repo.clone(),
        );
        let workspace_service = WorkspaceService::new(
            entry_repo.clone(),
            snapshot_repo.clone(),
            blob_store.clone(),
            upload_store.clone(),
        );
        let upload_service = UploadService::new(
            entry_repo.clone(),
            snapshot_repo.clone(),
            blob_store.clone(),
            upload_store.clone(),
        );

        let state = AppState {
            health_service: HealthService,
            session_service,
            auth_service: Some(auth_service),
            admin_service: Some(admin_service),
            search_service: SearchService::new(search_repo),
            share_service: ShareService::new(share_repo, entry_repo.clone()),
            history_service,
            workspace_service,
            upload_service,
            db_pool: pool.clone(),
            namespace_repo: Arc::new(namespace_repo),
            entry_repo: Arc::new(entry_repo),
            favorite_repo: Arc::new(vfiles_infra_sqlite::SqliteFavoriteRepo::new(pool.clone())),
            audit_service: vfiles_app::AuditService::new(
                vfiles_infra_sqlite::SqliteAuditLogRepo::new(pool.clone()),
            ),
            user_repo: Arc::new(vfiles_infra_sqlite::SqliteUserRepo::new(pool.clone())),
            access_token_service: vfiles_app::AccessTokenService::new(Arc::new(
                vfiles_infra_sqlite::SqliteAccessTokenRepo::new(pool.clone()),
            )),
            ownership_service: vfiles_app::OwnershipService::new(
                Arc::new(vfiles_infra_sqlite::SqliteEntryRepo::new(pool.clone())),
                Arc::new(vfiles_infra_sqlite::SqliteNamespaceRepo::new(pool.clone())),
                Arc::new(vfiles_infra_sqlite::SqliteUserRepo::new(pool.clone())),
                Arc::new(vfiles_infra_sqlite::SqliteSnapshotRepo::new(pool.clone())),
            ),
            snapshot_repo: Arc::new(snapshot_repo),
            blob_store: Arc::new(blob_store),
            upload_store: Arc::new(upload_store),
            login_attempt_limiter: Arc::new(LoginAttemptLimiter::new()),
            ingest_stats: Arc::new(vfiles_app::IngestStats::new()),
            share_download_limiter: Arc::new(vfiles_http::FixedWindowLimiter::new()),
            default_namespace_id,
            default_actor_user_id: admin_user_id,
            frontend_assets: frontend_dist
                .map(|path| path.into_std_path_buf())
                .and_then(FrontendAssets::filesystem),
            config,
        };

        Self {
            app: build_router(state),
            _temp_dir: temp_dir,
        }
    }

    async fn request(&self, request: Request<Body>) -> axum::response::Response {
        self.app
            .clone()
            .oneshot(request)
            .await
            .expect("request should succeed")
    }

    async fn json_request(
        &self,
        method: Method,
        uri: &str,
        body: Value,
    ) -> axum::response::Response {
        self.request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request should build"),
        )
        .await
    }

    async fn request_with_cookie(
        &self,
        mut request: Request<Body>,
        cookie: &str,
    ) -> axum::response::Response {
        request.headers_mut().insert(
            header::COOKIE,
            HeaderValue::from_str(cookie).expect("cookie header should be valid"),
        );
        self.request(request).await
    }

    async fn bytes_request_with_cookie(
        &self,
        method: Method,
        uri: &str,
        body: Vec<u8>,
        cookie: &str,
    ) -> axum::response::Response {
        self.request_with_cookie(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::from(body))
                .expect("request should build"),
            cookie,
        )
        .await
    }

    async fn json_request_with_cookie(
        &self,
        method: Method,
        uri: &str,
        body: Value,
        cookie: &str,
    ) -> axum::response::Response {
        self.request_with_cookie(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request should build"),
            cookie,
        )
        .await
    }

    async fn register_user(&self, username: &str, email: &str, password: &str) {
        let response = self
            .json_request(
                Method::POST,
                "/api/auth/register",
                json!({
                    "username": username,
                    "email": email,
                    "password": password,
                }),
            )
            .await;

        if response.status() != StatusCode::OK {
            panic!(
                "register failed with status {}: {}",
                response.status(),
                String::from_utf8_lossy(&response_bytes(response).await)
            );
        }
    }

    async fn register_user_without_email(&self, username: &str, password: &str) {
        let response = self
            .json_request(
                Method::POST,
                "/api/auth/register",
                json!({
                    "username": username,
                    "password": password,
                }),
            )
            .await;

        if response.status() != StatusCode::OK {
            panic!(
                "register failed with status {}: {}",
                response.status(),
                String::from_utf8_lossy(&response_bytes(response).await)
            );
        }
    }

    async fn login_cookie(&self, username_or_email: &str, password: &str) -> String {
        let response = self
            .json_request(
                Method::POST,
                "/api/auth/login",
                json!({
                    "username_or_email": username_or_email,
                    "password": password,
                }),
            )
            .await;

        if response.status() != StatusCode::OK {
            panic!(
                "login failed with status {}: {}",
                response.status(),
                String::from_utf8_lossy(&response_bytes(response).await)
            );
        }

        response
            .headers()
            .get(header::SET_COOKIE)
            .expect("set-cookie should be present")
            .to_str()
            .expect("set-cookie should be ascii")
            .split(';')
            .next()
            .expect("cookie pair should be present")
            .to_string()
    }

    async fn admin_cookie(&self) -> String {
        self.login_cookie("admin", "admin-password").await
    }

    async fn request_as_admin(&self, request: Request<Body>) -> axum::response::Response {
        let cookie = self.admin_cookie().await;
        self.request_with_cookie(request, &cookie).await
    }

    async fn json_request_as_admin(
        &self,
        method: Method,
        uri: &str,
        body: Value,
    ) -> axum::response::Response {
        let cookie = self.admin_cookie().await;
        self.json_request_with_cookie(method, uri, body, &cookie)
            .await
    }

    async fn bytes_request_as_admin(
        &self,
        method: Method,
        uri: &str,
        body: Vec<u8>,
    ) -> axum::response::Response {
        let cookie = self.admin_cookie().await;
        self.bytes_request_with_cookie(method, uri, body, &cookie)
            .await
    }

    async fn upload_version(
        &self,
        path: &str,
        filename: &str,
        bytes: &[u8],
        message: &str,
    ) -> String {
        let init_response = self
            .json_request_as_admin(
                Method::POST,
                "/api/files/upload/init",
                json!({
                    "path": path,
                    "filename": filename,
                    "size": bytes.len(),
                    "chunk_size": bytes.len(),
                }),
            )
            .await;
        if init_response.status() != StatusCode::OK {
            panic!(
                "upload init failed with status {}: {}",
                init_response.status(),
                String::from_utf8_lossy(&response_bytes(init_response).await)
            );
        }
        let init_payload = response_json(init_response).await;
        let upload_id = init_payload["upload_id"]
            .as_str()
            .expect("upload_id should be present");

        let chunk_response = self
            .bytes_request_as_admin(
                Method::PUT,
                &format!("/api/files/upload/chunks/{}/0", upload_id),
                bytes.to_vec(),
            )
            .await;
        if chunk_response.status() != StatusCode::OK {
            panic!(
                "upload chunk failed with status {}: {}",
                chunk_response.status(),
                String::from_utf8_lossy(&response_bytes(chunk_response).await)
            );
        }

        let complete_response = self
            .json_request_as_admin(
                Method::POST,
                &format!("/api/files/upload/complete/{}", upload_id),
                json!({ "message": message }),
            )
            .await;
        if complete_response.status() != StatusCode::OK {
            panic!(
                "upload complete failed with status {}: {}",
                complete_response.status(),
                String::from_utf8_lossy(&response_bytes(complete_response).await)
            );
        }
        let payload = response_json(complete_response).await;

        payload["version_id"]
            .as_str()
            .expect("version_id should be present")
            .to_string()
    }

    async fn upload_version_with_cookie(
        &self,
        cookie: &str,
        path: &str,
        filename: &str,
        bytes: &[u8],
        message: &str,
    ) -> String {
        let init_response = self
            .json_request_with_cookie(
                Method::POST,
                "/api/files/upload/init",
                json!({
                    "path": path,
                    "filename": filename,
                    "size": bytes.len(),
                    "chunk_size": bytes.len(),
                }),
                cookie,
            )
            .await;
        if init_response.status() != StatusCode::OK {
            panic!(
                "upload init failed with status {}: {}",
                init_response.status(),
                String::from_utf8_lossy(&response_bytes(init_response).await)
            );
        }
        let init_payload = response_json(init_response).await;
        let upload_id = init_payload["upload_id"]
            .as_str()
            .expect("upload_id should be present");

        let chunk_response = self
            .bytes_request_with_cookie(
                Method::PUT,
                &format!("/api/files/upload/chunks/{}/0", upload_id),
                bytes.to_vec(),
                cookie,
            )
            .await;
        if chunk_response.status() != StatusCode::OK {
            panic!(
                "upload chunk failed with status {}: {}",
                chunk_response.status(),
                String::from_utf8_lossy(&response_bytes(chunk_response).await)
            );
        }

        let complete_response = self
            .json_request_with_cookie(
                Method::POST,
                &format!("/api/files/upload/complete/{}", upload_id),
                json!({ "message": message }),
                cookie,
            )
            .await;
        if complete_response.status() != StatusCode::OK {
            panic!(
                "upload complete failed with status {}: {}",
                complete_response.status(),
                String::from_utf8_lossy(&response_bytes(complete_response).await)
            );
        }
        let payload = response_json(complete_response).await;

        payload["version_id"]
            .as_str()
            .expect("version_id should be present")
            .to_string()
    }
}

fn default_features() -> FeatureMatrix {
    FeatureMatrix {
        auth_enabled: true,
        multi_user: true,
        email_login: false,
        search_content: false,
        share_enabled: true,
        history_enabled: true,
        ftp_enabled: false,
        max_file_size_bytes: 4 * 1024 * 1024 * 1024,
    }
}

async fn response_bytes(response: axum::response::Response) -> bytes::Bytes {
    response
        .into_body()
        .collect()
        .await
        .expect("body should collect")
        .to_bytes()
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&response_bytes(response).await).expect("response should be valid json")
}

fn assert_gzip_json_response_headers(response: &axum::response::Response) {
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_ENCODING)
            .expect("content-encoding should be present"),
        "gzip"
    );

    let vary = response
        .headers()
        .get(header::VARY)
        .expect("vary should be present")
        .to_str()
        .expect("vary should be ascii");
    assert!(
        vary.split(',')
            .any(|value| value.trim().eq_ignore_ascii_case("accept-encoding")),
        "vary should mention accept-encoding, got: {vary}"
    );

    assert!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"))
    );
}

#[tokio::test]
async fn chunked_upload_history_and_download_round_trip() {
    let app = TestApp::new().await;

    let version1 = app
        .upload_version(
            "docs",
            "note.txt",
            b"hello from version one\n",
            "first version",
        )
        .await;
    let version2 = app
        .upload_version(
            "docs",
            "note.txt",
            b"hello from version two\n",
            "second version",
        )
        .await;

    let history = app
        .request_as_admin(
            Request::builder()
                .uri("/api/history?path=docs/note.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(history.status(), StatusCode::OK);
    let history_payload = response_json(history).await;
    assert_eq!(history_payload["success"], Value::Bool(true));
    assert_eq!(
        history_payload["data"]["currentVersion"],
        Value::String(version2.clone())
    );
    assert_eq!(history_payload["data"]["totalCommits"], Value::from(2));
    assert_eq!(
        history_payload["data"]["commits"][0]["author"]["name"],
        Value::String("admin".to_string())
    );
    assert_eq!(
        history_payload["data"]["commits"][1]["author"]["name"],
        Value::String("admin".to_string())
    );
    assert_eq!(
        history_payload["data"]["commits"][0]["changeType"],
        Value::String("modified".to_string())
    );
    assert_eq!(
        history_payload["data"]["commits"][0]["hasCustomMessage"],
        Value::Bool(true)
    );
    assert_eq!(
        history_payload["data"]["commits"][1]["changeType"],
        Value::String("added".to_string())
    );
    assert_eq!(
        history_payload["data"]["commits"][1]["hasCustomMessage"],
        Value::Bool(true)
    );
    let history_date = history_payload["data"]["commits"][0]["date"]
        .as_str()
        .expect("history commit date should be present");
    time::OffsetDateTime::parse(history_date, &Rfc3339)
        .expect("history commit date should be RFC3339");

    let current_content = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/content?path=docs/note.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(current_content.status(), StatusCode::OK);
    assert_eq!(
        response_bytes(current_content).await.as_ref(),
        b"hello from version two\n"
    );

    let old_content = app
        .request_as_admin(
            Request::builder()
                .uri(format!(
                    "/api/files/content?path=docs/note.txt&commit={}",
                    version1
                ))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(old_content.status(), StatusCode::OK);
    assert_eq!(
        response_bytes(old_content).await.as_ref(),
        b"hello from version one\n"
    );

    let diff = app
        .request_as_admin(
            Request::builder()
                .uri(format!(
                    "/api/history/diff?path=docs/note.txt&commit={}",
                    version2
                ))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(diff.status(), StatusCode::OK);
    let diff_text =
        String::from_utf8(response_bytes(diff).await.to_vec()).expect("diff should be utf-8");
    assert!(diff_text.contains("hello from version one"));
    assert!(diff_text.contains("hello from version two"));

    let file_download = app
        .request_as_admin(
            Request::builder()
                .uri("/api/download?path=docs/note.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(file_download.status(), StatusCode::OK);
    assert_eq!(
        response_bytes(file_download).await.as_ref(),
        b"hello from version two\n"
    );

    let folder_download = app
        .request_as_admin(
            Request::builder()
                .uri("/api/download/folder?path=docs")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(folder_download.status(), StatusCode::OK);
    assert_eq!(
        folder_download.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/zip"))
    );
    let archive_size = folder_download
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .expect("archive response should declare its length");
    let zip_bytes = response_bytes(folder_download).await;
    assert_eq!(zip_bytes.len(), archive_size);
    let cursor = Cursor::new(zip_bytes.to_vec());
    let mut archive = zip::ZipArchive::new(cursor).expect("zip archive should open");
    assert_eq!(archive.len(), 1);

    let mut file = archive
        .by_name("docs/note.txt")
        .expect("zip should contain uploaded file");
    let mut extracted = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut extracted).expect("zip entry should read");
    assert_eq!(extracted, b"hello from version two\n");

    let ranged_folder_download = app
        .request_as_admin(
            Request::builder()
                .uri("/api/download/folder?path=docs")
                .header(header::RANGE, "bytes=0-3")
                .body(Body::empty())
                .expect("range request should build"),
        )
        .await;
    assert_eq!(ranged_folder_download.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        ranged_folder_download
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok()),
        Some(format!("bytes 0-3/{archive_size}").as_str())
    );
    assert_eq!(
        response_bytes(ranged_folder_download).await.as_ref(),
        &zip_bytes[..4]
    );

    let versioned_folder_download = app
        .request_as_admin(
            Request::builder()
                .uri(format!(
                    "/api/download/folder?path=docs&commit={}",
                    version1
                ))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(versioned_folder_download.status(), StatusCode::OK);
    let versioned_zip_bytes = response_bytes(versioned_folder_download).await;
    let versioned_cursor = Cursor::new(versioned_zip_bytes.to_vec());
    let mut versioned_archive =
        zip::ZipArchive::new(versioned_cursor).expect("versioned zip archive should open");
    let mut versioned_file = versioned_archive
        .by_name("docs/note.txt")
        .expect("versioned zip should contain uploaded file");
    let mut versioned_extracted = Vec::new();
    std::io::Read::read_to_end(&mut versioned_file, &mut versioned_extracted)
        .expect("versioned zip entry should read");
    assert_eq!(versioned_extracted, b"hello from version one\n");

    let snapshot = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/snapshots",
            json!({ "message": "docs snapshot" }),
        )
        .await;
    assert_eq!(snapshot.status(), StatusCode::OK);
    let snapshot_payload = response_json(snapshot).await;
    let snapshot_id = snapshot_payload["id"]
        .as_str()
        .expect("snapshot id should be present");

    app.upload_version(
        "docs",
        "note.txt",
        b"hello from version three\n",
        "third version",
    )
    .await;

    let snapshot_folder_download = app
        .request_as_admin(
            Request::builder()
                .uri(format!(
                    "/api/download/folder?path=docs&commit={}",
                    snapshot_id
                ))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(snapshot_folder_download.status(), StatusCode::OK);
    let snapshot_zip_bytes = response_bytes(snapshot_folder_download).await;
    let snapshot_cursor = Cursor::new(snapshot_zip_bytes.to_vec());
    let mut snapshot_archive =
        zip::ZipArchive::new(snapshot_cursor).expect("snapshot zip archive should open");
    let mut snapshot_file = snapshot_archive
        .by_name("docs/note.txt")
        .expect("snapshot zip should contain uploaded file");
    let mut snapshot_extracted = Vec::new();
    std::io::Read::read_to_end(&mut snapshot_file, &mut snapshot_extracted)
        .expect("snapshot zip entry should read");
    assert_eq!(snapshot_extracted, b"hello from version two\n");
}

#[tokio::test]
async fn file_content_and_download_support_range_requests() {
    let app = TestApp::new().await;
    app.upload_version("docs", "range.txt", b"0123456789", "range upload")
        .await;

    let content_partial = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/content?path=docs/range.txt")
                .header(header::RANGE, "bytes=2-5")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(content_partial.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        content_partial
            .headers()
            .get(header::ACCEPT_RANGES)
            .expect("accept-ranges should be present"),
        "bytes"
    );
    assert_eq!(
        content_partial
            .headers()
            .get(header::CONTENT_RANGE)
            .expect("content-range should be present"),
        "bytes 2-5/10"
    );
    assert_eq!(
        content_partial
            .headers()
            .get(header::CONTENT_LENGTH)
            .expect("content-length should be present"),
        "4"
    );
    assert_eq!(response_bytes(content_partial).await.as_ref(), b"2345");

    let content_suffix = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/content?path=docs/range.txt")
                .header(header::RANGE, "bytes=-3")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(content_suffix.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        content_suffix
            .headers()
            .get(header::CONTENT_RANGE)
            .expect("content-range should be present"),
        "bytes 7-9/10"
    );
    assert_eq!(response_bytes(content_suffix).await.as_ref(), b"789");

    let download_partial = app
        .request_as_admin(
            Request::builder()
                .uri("/api/download?path=docs/range.txt")
                .header(header::RANGE, "bytes=0-1")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(download_partial.status(), StatusCode::PARTIAL_CONTENT);
    assert!(
        download_partial
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .is_some(),
        "content-disposition should be present"
    );
    assert_eq!(response_bytes(download_partial).await.as_ref(), b"01");

    let unsatisfied = app
        .request_as_admin(
            Request::builder()
                .uri("/api/download?path=docs/range.txt")
                .header(header::RANGE, "bytes=99-")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(unsatisfied.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        unsatisfied
            .headers()
            .get(header::CONTENT_RANGE)
            .expect("content-range should be present"),
        "bytes */10"
    );
}

#[tokio::test]
async fn history_restore_endpoint_creates_new_current_version() {
    let app = TestApp::new().await;

    let version1 = app
        .upload_version("docs", "note.txt", b"version one\n", "first version")
        .await;
    let version2 = app
        .upload_version("docs", "note.txt", b"version two\n", "second version")
        .await;

    let restore_response = app
        .json_request_as_admin(
            Method::POST,
            "/api/history/restore",
            json!({
                "path": "docs/note.txt",
                "commit": version1,
                "message": "恢复到版本 one"
            }),
        )
        .await;
    assert_eq!(restore_response.status(), StatusCode::OK);
    let restore_payload = response_json(restore_response).await;
    assert_eq!(restore_payload["success"], Value::Bool(true));
    assert_eq!(
        restore_payload["data"]["path"],
        Value::String("docs/note.txt".to_string())
    );
    assert_eq!(
        restore_payload["data"]["restoredFrom"],
        Value::String(version1.clone())
    );
    assert!(restore_payload["data"]["snapshotId"].is_string());
    let restored_current = restore_payload["data"]["currentVersion"]
        .as_str()
        .expect("currentVersion should be present")
        .to_string();
    assert_ne!(restored_current, version2);

    let current_content = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/content?path=docs/note.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(current_content.status(), StatusCode::OK);
    assert_eq!(
        response_bytes(current_content).await.as_ref(),
        b"version one\n"
    );

    let history = app
        .request_as_admin(
            Request::builder()
                .uri("/api/history?path=docs/note.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(history.status(), StatusCode::OK);
    let history_payload = response_json(history).await;
    assert_eq!(
        history_payload["data"]["currentVersion"],
        Value::String(restored_current)
    );
    assert_eq!(
        history_payload["data"]["commits"][0]["message"],
        Value::String("恢复到版本 one".to_string())
    );
    assert_eq!(
        history_payload["data"]["commits"][0]["changeType"],
        Value::String("modified".to_string())
    );
    assert_eq!(
        history_payload["data"]["commits"][0]["hasCustomMessage"],
        Value::Bool(true)
    );
}

#[tokio::test]
async fn single_upload_streams_multipart_file_to_storage() {
    let app = TestApp::new().await;
    let payload = b"streaming single upload content\n";
    let (body, content_type) =
        single_upload_multipart("streamed.txt", "docs", "streamed upload", payload);

    let response = app
        .request_as_admin(
            Request::builder()
                .method(Method::POST)
                .uri("/api/files/upload")
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let payload_json = response_json(response).await;
    assert_eq!(payload_json["completed"], Value::Bool(true));
    assert_eq!(payload_json["size"], Value::from(payload.len()));

    let content = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/content?path=docs/streamed.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(response_bytes(content).await.as_ref(), payload);
}

#[tokio::test]
async fn tree_lists_direct_children_and_file_metadata() {
    let app = TestApp::new().await;

    let root_dir = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "docs" }),
        )
        .await;
    assert_eq!(root_dir.status(), StatusCode::OK);

    let nested_dir = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "docs/nested" }),
        )
        .await;
    assert_eq!(nested_dir.status(), StatusCode::OK);

    app.upload_version("docs", "root.txt", b"root file\n", "root upload")
        .await;
    app.upload_version("docs/nested", "child.txt", b"child file\n", "child upload")
        .await;

    let root_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(root_listing.status(), StatusCode::OK);
    let root_entries = response_json(root_listing).await;
    assert_eq!(root_entries.as_array().expect("array response").len(), 1);
    assert_eq!(root_entries[0]["path"], Value::String("docs".to_string()));

    let docs_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/docs")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(docs_listing.status(), StatusCode::OK);
    let docs_entries = response_json(docs_listing).await;
    let entries = docs_entries.as_array().expect("array response");
    assert_eq!(entries.len(), 2);

    let file_entry = entries
        .iter()
        .find(|entry| entry["path"] == Value::String("docs/root.txt".to_string()))
        .expect("file entry should be present");
    assert_eq!(file_entry["kind"], Value::String("file".to_string()));
    assert_eq!(file_entry["size_bytes"], Value::from(10));
    assert_eq!(
        file_entry["mime_type"],
        Value::String("text/plain".to_string())
    );
    assert_eq!(file_entry["is_text"], Value::Bool(true));
    assert!(file_entry["updated_at"].is_string());
    time::OffsetDateTime::parse(
        file_entry["created_at"]
            .as_str()
            .expect("created_at should be string"),
        &Rfc3339,
    )
    .expect("created_at should be RFC3339");
    time::OffsetDateTime::parse(
        file_entry["updated_at"]
            .as_str()
            .expect("updated_at should be string"),
        &Rfc3339,
    )
    .expect("updated_at should be RFC3339");

    let dir_entry = entries
        .iter()
        .find(|entry| entry["path"] == Value::String("docs/nested".to_string()))
        .expect("nested directory should be present");
    assert_eq!(dir_entry["kind"], Value::String("directory".to_string()));
}

#[tokio::test]
async fn tree_does_not_surface_grandchildren_in_parent_listing() {
    let app = TestApp::new().await;

    for path in ["foo", "foo/bar", "foo/bar/aka"] {
        let response = app
            .json_request_as_admin(
                Method::POST,
                "/api/files/directories",
                json!({ "path": path }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK, "failed to create {path}");
    }

    let foo_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/foo")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(foo_listing.status(), StatusCode::OK);
    let foo_entries = response_json(foo_listing).await;
    let foo_entries = foo_entries
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(foo_entries.len(), 1);
    assert_eq!(foo_entries[0]["path"], Value::String("foo/bar".to_string()));

    let bar_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/foo/bar")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(bar_listing.status(), StatusCode::OK);
    let bar_entries = response_json(bar_listing).await;
    let bar_entries = bar_entries
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(bar_entries.len(), 1);
    assert_eq!(
        bar_entries[0]["path"],
        Value::String("foo/bar/aka".to_string())
    );
}

#[tokio::test]
async fn create_directory_materializes_missing_parent_directories() {
    let app = TestApp::new().await;

    let create_response = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "foo/bar/aka" }),
        )
        .await;
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_payload = response_json(create_response).await;
    assert_eq!(
        create_payload["path"],
        Value::String("foo/bar/aka".to_string())
    );
    assert_eq!(
        create_payload["kind"],
        Value::String("directory".to_string())
    );

    let root_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(root_listing.status(), StatusCode::OK);
    let root_entries = response_json(root_listing).await;
    let root_entries = root_entries
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(root_entries.len(), 1);
    assert_eq!(root_entries[0]["path"], Value::String("foo".to_string()));

    let foo_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/foo")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(foo_listing.status(), StatusCode::OK);
    let foo_entries = response_json(foo_listing).await;
    let foo_entries = foo_entries
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(foo_entries.len(), 1);
    assert_eq!(foo_entries[0]["path"], Value::String("foo/bar".to_string()));

    let bar_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/foo/bar")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(bar_listing.status(), StatusCode::OK);
    let bar_entries = response_json(bar_listing).await;
    let bar_entries = bar_entries
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(bar_entries.len(), 1);
    assert_eq!(
        bar_entries[0]["path"],
        Value::String("foo/bar/aka".to_string())
    );
}

#[tokio::test]
async fn create_directory_with_unicode_path_keeps_tree_structure_consistent() {
    let app = TestApp::new().await;

    let create_response = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "数据集/细对接/01数据" }),
        )
        .await;
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_payload = response_json(create_response).await;
    assert_eq!(
        create_payload["path"],
        Value::String("数据集/细对接/01数据".to_string())
    );

    let dataset_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/数据集")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(dataset_listing.status(), StatusCode::OK);
    let dataset_entries = response_json(dataset_listing).await;
    let dataset_entries = dataset_entries
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(dataset_entries.len(), 1);
    assert_eq!(
        dataset_entries[0]["path"],
        Value::String("数据集/细对接".to_string())
    );

    let integration_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/数据集/细对接")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(integration_listing.status(), StatusCode::OK);
    let integration_entries = response_json(integration_listing).await;
    let integration_entries = integration_entries
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(integration_entries.len(), 1);
    assert_eq!(
        integration_entries[0]["path"],
        Value::String("数据集/细对接/01数据".to_string())
    );
}

#[tokio::test]
async fn delete_route_removes_entry_from_live_tree() {
    let app = TestApp::new().await;

    let create_dir = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "docs" }),
        )
        .await;
    assert_eq!(create_dir.status(), StatusCode::OK);

    app.upload_version("docs", "delete-me.txt", b"remove me\n", "seed delete")
        .await;
    app.upload_version("docs", "keep.txt", b"keep me\n", "seed keep")
        .await;

    let delete_response = app
        .request_as_admin(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/files?path=docs/delete-me.txt&message=delete")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let listing_response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/docs")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(listing_response.status(), StatusCode::OK);
    let listing_payload = response_json(listing_response).await;
    let entries = listing_payload
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0]["path"],
        Value::String("docs/keep.txt".to_string())
    );

    let deleted_content = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/content?path=docs/delete-me.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(deleted_content.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn move_route_moves_entry_between_directories() {
    let app = TestApp::new().await;

    let create_docs = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "docs" }),
        )
        .await;
    assert_eq!(create_docs.status(), StatusCode::OK);

    let create_archive = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "archive" }),
        )
        .await;
    assert_eq!(create_archive.status(), StatusCode::OK);

    app.upload_version("docs", "note.txt", b"move me\n", "seed move")
        .await;

    let move_response = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/move",
            json!({
                "from": "docs/note.txt",
                "to": "archive/note.txt",
                "message": "move file"
            }),
        )
        .await;
    assert_eq!(move_response.status(), StatusCode::NO_CONTENT);

    let docs_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/docs")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(docs_listing.status(), StatusCode::OK);
    let docs_payload = response_json(docs_listing).await;
    assert_eq!(
        docs_payload
            .as_array()
            .expect("docs response should be array")
            .len(),
        0
    );

    let archive_listing = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/tree/archive")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(archive_listing.status(), StatusCode::OK);
    let archive_payload = response_json(archive_listing).await;
    let archive_entries = archive_payload
        .as_array()
        .expect("archive response should be array");
    assert_eq!(archive_entries.len(), 1);
    assert_eq!(
        archive_entries[0]["path"],
        Value::String("archive/note.txt".to_string())
    );
}

#[tokio::test]
async fn multi_user_mode_isolates_tree_and_content_by_authenticated_user() {
    let app = TestApp::new().await;

    app.register_user("alice", "alice@example.com", "alice-password")
        .await;
    app.register_user("bob", "bob@example.com", "bob-password")
        .await;

    let alice_cookie = app.login_cookie("alice", "alice-password").await;
    let bob_cookie = app.login_cookie("bob", "bob-password").await;

    app.upload_version_with_cookie(
        &alice_cookie,
        "docs",
        "alice.txt",
        b"alice private\n",
        "alice upload",
    )
    .await;
    app.upload_version_with_cookie(
        &bob_cookie,
        "notes",
        "bob.txt",
        b"bob private\n",
        "bob upload",
    )
    .await;

    let alice_root = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/tree")
                .body(Body::empty())
                .expect("request should build"),
            &alice_cookie,
        )
        .await;
    assert_eq!(alice_root.status(), StatusCode::OK);
    let alice_entries = response_json(alice_root).await;
    let alice_paths: Vec<&str> = alice_entries
        .as_array()
        .expect("tree response should be array")
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .collect();
    assert_eq!(alice_paths, vec!["docs"]);

    let bob_root = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/tree")
                .body(Body::empty())
                .expect("request should build"),
            &bob_cookie,
        )
        .await;
    assert_eq!(bob_root.status(), StatusCode::OK);
    let bob_entries = response_json(bob_root).await;
    let bob_paths: Vec<&str> = bob_entries
        .as_array()
        .expect("tree response should be array")
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .collect();
    assert_eq!(bob_paths, vec!["notes"]);

    let alice_content = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/content?path=docs/alice.txt")
                .body(Body::empty())
                .expect("request should build"),
            &alice_cookie,
        )
        .await;
    assert_eq!(alice_content.status(), StatusCode::OK);
    assert_eq!(
        response_bytes(alice_content).await.as_ref(),
        b"alice private\n"
    );

    let bob_access_alice = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/content?path=docs/alice.txt")
                .body(Body::empty())
                .expect("request should build"),
            &bob_cookie,
        )
        .await;
    assert_eq!(bob_access_alice.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_snapshot_returns_incrementing_snapshot_metadata() {
    let app = TestApp::new().await;

    let first = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/snapshots",
            json!({ "message": "checkpoint one" }),
        )
        .await;
    assert_eq!(first.status(), StatusCode::OK);
    let first_payload = response_json(first).await;
    assert_eq!(first_payload["snapshot_no"], Value::from(1));
    assert_eq!(
        first_payload["message"],
        Value::String("checkpoint one".to_string())
    );
    assert_eq!(first_payload["kind"], Value::String("user".to_string()));

    let second = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/snapshots",
            json!({ "message": "checkpoint two" }),
        )
        .await;
    assert_eq!(second.status(), StatusCode::OK);
    let second_payload = response_json(second).await;
    assert_eq!(second_payload["snapshot_no"], Value::from(2));
    assert_eq!(
        second_payload["message"],
        Value::String("checkpoint two".to_string())
    );
}

#[tokio::test]
async fn directory_history_returns_snapshot_commits_for_tree_and_content_browsing() {
    let app = TestApp::new().await;

    let create_dir = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "docs" }),
        )
        .await;
    assert_eq!(create_dir.status(), StatusCode::OK);

    app.upload_version("docs", "note.txt", b"v1\n", "seed version")
        .await;

    let snapshot = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/snapshots",
            json!({ "message": "docs snapshot" }),
        )
        .await;
    assert_eq!(snapshot.status(), StatusCode::OK);
    let snapshot_payload = response_json(snapshot).await;
    let snapshot_id = snapshot_payload["id"]
        .as_str()
        .expect("snapshot id should be present")
        .to_string();

    app.upload_version("docs", "note.txt", b"v2\n", "updated version")
        .await;

    let history = app
        .request_as_admin(
            Request::builder()
                .uri("/api/history?path=docs")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(history.status(), StatusCode::OK);
    let history_payload = response_json(history).await;
    assert!(
        history_payload["data"]["commits"]
            .as_array()
            .expect("commits should be an array")
            .iter()
            .any(|commit| commit["hash"] == Value::String(snapshot_id.clone()))
    );

    let tree = app
        .request_as_admin(
            Request::builder()
                .uri(format!("/api/files/tree/docs?commit={}", snapshot_id))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(tree.status(), StatusCode::OK);
    let tree_payload = response_json(tree).await;
    let entries = tree_payload
        .as_array()
        .expect("tree response should be an array");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0]["path"],
        Value::String("docs/note.txt".to_string())
    );
    assert_eq!(entries[0]["size_bytes"], Value::from(3));

    let content = app
        .request_as_admin(
            Request::builder()
                .uri(format!(
                    "/api/files/content?path=docs/note.txt&commit={}",
                    snapshot_id
                ))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(response_bytes(content).await.as_ref(), b"v1\n");
}

#[tokio::test]
async fn session_bootstrap_and_admin_routes_require_authenticated_admin() {
    let app = TestApp::new().await;

    let anonymous_admin = app
        .request(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(anonymous_admin.status(), StatusCode::UNAUTHORIZED);

    app.register_user("member", "member@example.com", "member-password")
        .await;
    let member_cookie = app.login_cookie("member", "member-password").await;
    let member_admin = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
            &member_cookie,
        )
        .await;
    assert_eq!(member_admin.status(), StatusCode::FORBIDDEN);

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let bootstrap = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/session/bootstrap")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(bootstrap.status(), StatusCode::OK);
    let bootstrap_payload = response_json(bootstrap).await;
    assert_eq!(bootstrap_payload["auth_enabled"], Value::Bool(true));
    assert!(bootstrap_payload["current_user"].is_string());
    assert!(bootstrap_payload["active_workspace"].is_string());
    assert!(
        bootstrap_payload["capabilities"]
            .as_array()
            .expect("capabilities should be an array")
            .iter()
            .any(|value| value == "admin_users")
    );

    let admin_users = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(admin_users.status(), StatusCode::OK);
    let admin_users_payload = response_json(admin_users).await;
    assert_eq!(admin_users_payload["total_count"], Value::from(2));
    assert_eq!(
        admin_users_payload["users"]
            .as_array()
            .expect("users should be an array")
            .len(),
        2
    );
}

#[tokio::test]
async fn login_accepts_camel_case_username_field() {
    let app = TestApp::new().await;

    let response = app
        .json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "usernameOrEmail": "admin",
                "password": "admin-password",
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(header::SET_COOKIE).is_some());
}

#[tokio::test]
async fn login_for_unknown_user_returns_invalid_credentials() {
    let app = TestApp::new().await;

    let response = app
        .json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "username_or_email": "ghostuser",
                "password": "wrong-password",
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let payload = response_json(response).await;
    assert_eq!(
        payload["code"],
        Value::String("INVALID_CREDENTIALS".to_string())
    );
}

#[tokio::test]
async fn login_with_invalid_identifier_format_returns_invalid_credentials() {
    let app = TestApp::new().await;

    let response = app
        .json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "username_or_email": "not-an-email@",
                "password": "wrong-password",
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let payload = response_json(response).await;
    assert_eq!(
        payload["code"],
        Value::String("INVALID_CREDENTIALS".to_string())
    );
}

#[tokio::test]
async fn login_rate_limit_blocks_repeated_failed_attempts() {
    let app = TestApp::new_with_login_rate_limit(3).await;

    for _ in 0..3 {
        let response = app
            .request(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-forwarded-for", "203.0.113.10")
                    .body(Body::from(
                        json!({
                            "username_or_email": "admin",
                            "password": "wrong-password",
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let blocked = app
        .request(
            Request::builder()
                .method(Method::POST)
                .uri("/api/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", "203.0.113.10")
                .body(Body::from(
                    json!({
                        "username_or_email": "admin",
                        "password": "wrong-password",
                    })
                    .to_string(),
                ))
                .expect("request should build"),
        )
        .await;

    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(blocked.headers().get(header::RETRY_AFTER).is_some());
    let payload = response_json(blocked).await;
    assert_eq!(
        payload["code"],
        Value::String("LOGIN_RATE_LIMITED".to_string())
    );
}

#[tokio::test]
async fn successful_login_clears_failed_attempt_counter() {
    let app = TestApp::new_with_login_rate_limit(3).await;

    for _ in 0..2 {
        let response = app
            .request(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-forwarded-for", "203.0.113.10")
                    .body(Body::from(
                        json!({
                            "username_or_email": "admin",
                            "password": "wrong-password",
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let successful_login = app
        .request(
            Request::builder()
                .method(Method::POST)
                .uri("/api/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", "203.0.113.10")
                .body(Body::from(
                    json!({
                        "username_or_email": "admin",
                        "password": "admin-password",
                    })
                    .to_string(),
                ))
                .expect("request should build"),
        )
        .await;

    assert_eq!(successful_login.status(), StatusCode::OK);

    let failed_after_success = app
        .request(
            Request::builder()
                .method(Method::POST)
                .uri("/api/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", "203.0.113.10")
                .body(Body::from(
                    json!({
                        "username_or_email": "admin",
                        "password": "wrong-password",
                    })
                    .to_string(),
                ))
                .expect("request should build"),
        )
        .await;

    assert_eq!(failed_after_success.status(), StatusCode::UNAUTHORIZED);
    assert!(
        failed_after_success
            .headers()
            .get(header::RETRY_AFTER)
            .is_none()
    );
}

#[tokio::test]
async fn cors_preflight_allows_configured_public_origin_with_credentials() {
    let app = TestApp::new().await;

    let allowed = app
        .request(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/health")
                .header(header::ORIGIN, "http://example.test:4242")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert!(allowed.status().is_success());
    assert_eq!(
        allowed
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .expect("allow-origin should be present"),
        "http://example.test:4242"
    );
    assert_eq!(
        allowed
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .expect("allow-credentials should be present"),
        "true"
    );

    let blocked = app
        .request(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/health")
                .header(header::ORIGIN, "http://evil.test")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert!(
        blocked
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[tokio::test]
async fn health_json_response_supports_gzip_when_requested() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_gzip_json_response_headers(&response);

    let body = response_bytes(response).await;
    assert!(
        !body.is_empty(),
        "compressed response body should not be empty"
    );
}

#[tokio::test]
async fn request_id_and_security_headers_are_applied() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/api/auth/me")
                .header("x-request-id", "test-request-123")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .expect("request id header should be present"),
        "test-request-123"
    );
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .expect("content type options header should be present"),
        "nosniff"
    );
    assert_eq!(
        response
            .headers()
            .get("x-frame-options")
            .expect("frame options header should be present"),
        "SAMEORIGIN"
    );
    assert_eq!(
        response
            .headers()
            .get("referrer-policy")
            .expect("referrer policy header should be present"),
        "strict-origin-when-cross-origin"
    );
}

#[tokio::test]
async fn admin_errors_return_structured_json_with_request_id() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/api/admin/users")
                .header("x-request-id", "admin-request-123")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .expect("request id header should be present"),
        "admin-request-123"
    );

    let payload = response_json(response).await;
    assert_eq!(payload["code"], Value::String("UNAUTHORIZED".to_string()));
    assert_eq!(
        payload["request_id"],
        Value::String("admin-request-123".to_string())
    );
}

#[tokio::test]
async fn tree_json_response_supports_gzip_when_requested() {
    let app = TestApp::new().await;

    app.upload_version("docs", "root.txt", b"root file\n", "root upload")
        .await;
    app.upload_version("docs/nested", "child.txt", b"child file\n", "child upload")
        .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .method(Method::GET)
                .uri("/api/files/tree/docs")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_gzip_json_response_headers(&response);

    let body = response_bytes(response).await;
    assert!(
        !body.is_empty(),
        "compressed response body should not be empty"
    );
}

#[tokio::test]
async fn history_json_response_supports_gzip_when_requested() {
    let app = TestApp::new().await;

    app.upload_version(
        "docs",
        "note.txt",
        b"hello from version one\n",
        "first version",
    )
    .await;
    app.upload_version(
        "docs",
        "note.txt",
        b"hello from version two\n",
        "second version",
    )
    .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .method(Method::GET)
                .uri("/api/history?path=docs/note.txt")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_gzip_json_response_headers(&response);

    let body = response_bytes(response).await;
    assert!(
        !body.is_empty(),
        "compressed response body should not be empty"
    );
}

#[tokio::test]
async fn admin_users_json_response_supports_gzip_when_requested() {
    let app = TestApp::new().await;

    app.register_user("member", "member@example.com", "member-password")
        .await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    let response = app
        .request_with_cookie(
            Request::builder()
                .method(Method::GET)
                .uri("/api/admin/users")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;

    assert_gzip_json_response_headers(&response);

    let body = response_bytes(response).await;
    assert!(
        !body.is_empty(),
        "compressed response body should not be empty"
    );
}

#[tokio::test]
async fn search_json_response_supports_gzip_when_requested() {
    let mut features = default_features();
    features.search_content = true;
    let app = TestApp::new_with_features(features).await;

    app.upload_version(
        "docs",
        "notes.txt",
        b"alpha\nhello streaming search\nomega\n",
        "seed search content",
    )
    .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .method(Method::GET)
                .uri("/api/files/search?q=streaming&search_content=true")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_gzip_json_response_headers(&response);

    let body = response_bytes(response).await;
    assert!(
        !body.is_empty(),
        "compressed response body should not be empty"
    );
}

#[tokio::test]
async fn login_sets_secure_cookie_for_https_public_base_url() {
    let app = TestApp::new_with_public_base_url("https://example.test:4242").await;

    let response = app
        .json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "username_or_email": "admin",
                "password": "admin-password",
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("set-cookie should be present")
        .to_str()
        .expect("set-cookie should be ascii");
    assert!(set_cookie.contains("Secure"));
}

#[tokio::test]
async fn login_can_disable_secure_cookie_for_https_public_base_url() {
    let app = TestApp::new_with_public_base_url_and_cookie_secure(
        "https://example.test:4242",
        Some(false),
    )
    .await;

    let response = app
        .json_request(
            Method::POST,
            "/api/auth/login",
            json!({
                "username_or_email": "admin",
                "password": "admin-password",
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("set-cookie should be present")
        .to_str()
        .expect("set-cookie should be ascii");
    assert!(!set_cookie.contains("Secure"));
}

#[tokio::test]
async fn auth_me_returns_wrapped_state_for_refresh_recovery() {
    let app = TestApp::new().await;

    let anonymous = app
        .request(
            Request::builder()
                .uri("/api/auth/me")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(anonymous.status(), StatusCode::OK);
    let anonymous_payload = response_json(anonymous).await;
    assert_eq!(anonymous_payload["success"], Value::Bool(true));
    assert_eq!(anonymous_payload["data"]["enabled"], Value::Bool(true));
    assert_eq!(
        anonymous_payload["data"]["allowRegister"],
        Value::Bool(true)
    );
    assert_eq!(anonymous_payload["data"]["user"], Value::Null);

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let current = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/auth/me")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(current.status(), StatusCode::OK);
    let current_payload = response_json(current).await;
    assert_eq!(current_payload["success"], Value::Bool(true));
    assert_eq!(current_payload["data"]["enabled"], Value::Bool(true));
    assert_eq!(
        current_payload["data"]["user"]["username"],
        Value::String("admin".to_string())
    );
    assert_eq!(
        current_payload["data"]["user"]["role"],
        Value::String("admin".to_string())
    );
}

#[tokio::test]
async fn share_creation_uses_configured_public_base_url() {
    let app = TestApp::new().await;
    app.upload_version("docs", "share.txt", b"shared file\n", "share source")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let share = app
        .json_request_with_cookie(
            Method::POST,
            "/api/share/shares",
            json!({ "path": "docs/share.txt" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(share.status(), StatusCode::OK);

    let share_payload = response_json(share).await;
    let code = share_payload["code"]
        .as_str()
        .expect("share code should be present");
    assert_eq!(
        share_payload["share_url"],
        Value::String(format!("http://example.test:4242/s/{}", code))
    );
}

/// 已过期的分享，所有者仍然可以停止（否则过期链接无法清理）。
#[tokio::test]
async fn expired_share_can_still_be_disabled_by_owner() {
    let app = TestApp::new().await;
    app.upload_version("docs", "过期.txt", b"expired\n", "seed")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let created = app
        .json_request_with_cookie(
            Method::POST,
            "/api/share/shares",
            json!({ "path": "docs/过期.txt", "expires_at": "2020-01-01T00:00:00Z" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(created.status(), StatusCode::OK);
    let code = response_json(created).await["code"]
        .as_str()
        .expect("share code")
        .to_string();

    // 匿名访问已过期链接应被拒绝
    let access = app
        .request(
            Request::builder()
                .uri(format!("/api/share/shares/{code}"))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(access.status(), StatusCode::NOT_FOUND);

    // 但所有者可以停止它
    let disabled = app
        .request_with_cookie(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/share/shares/{code}"))
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(
        disabled.status(),
        StatusCode::NO_CONTENT,
        "过期链接也应能被所有者停止"
    );

    // 停止后不再出现在列表里
    let list = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/share/shares")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let payload = response_json(list).await;
    assert_eq!(
        payload.as_array().map(Vec::len),
        Some(0),
        "停止后列表应为空: {payload:?}"
    );
}

/// 上传必须按单文件配置上限拒绝（此前只在客户端预检）。
#[tokio::test]
async fn upload_rejects_files_over_the_configured_limit() {
    let app = TestApp::new_with_max_file_size_bytes(1024 * 1024).await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    // 测试配置的单文件上限是 1MB：2MB 应被拒绝
    let oversized = vec![b'x'; 2 * 1024 * 1024];
    let upload = app
        .request_with_cookie(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/files/upload/too-big.bin")
                .body(Body::from(oversized.clone()))
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(
        upload.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "超过单文件上限应返回 413"
    );
    let payload = response_json(upload).await;
    assert!(
        payload["message"]
            .as_str()
            .is_some_and(|message| message.contains("1048576")),
        "错误信息应包含上限字节数: {payload:?}"
    );

    // 分片上传同样按该上限拒绝
    let init = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/upload/init",
            json!({
                "path": "",
                "filename": "too-big-2.bin",
                "size": 2 * 1024 * 1024,
                "chunk_size": 2 * 1024 * 1024,
            }),
        )
        .await;
    // 分片上传在 init 阶段用校验错误拒绝（沿用既有行为与文案）
    assert_eq!(
        init.status(),
        StatusCode::BAD_REQUEST,
        "分片上传的 init 也应拒绝超限文件"
    );
    let init_payload = response_json(init).await;
    assert!(
        init_payload["message"]
            .as_str()
            .is_some_and(|message| message.contains("1048576")),
        "init 的错误信息应包含上限字节数: {init_payload:?}"
    );

    // 1MB 以内正常
    let allowed = app
        .request_with_cookie(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/files/upload/ok.bin")
                .body(Body::from(vec![b'y'; 1024]))
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(allowed.status(), StatusCode::OK);
}

/// 单请求上传（原始 body）：curl 友好的上传方式，无需 init/分片。
#[tokio::test]
async fn put_upload_accepts_raw_body() {
    let app = TestApp::new().await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    // 1) 指定目录 + 文件名
    let upload = app
        .request_with_cookie(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/files/upload?path=ci&filename=%E6%9E%84%E5%BB%BA%E4%BA%A7%E7%89%A9.txt&message=CI%20%E4%B8%8A%E4%BC%A0")
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::from("hello build".as_bytes().to_vec()))
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(
        upload.status(),
        StatusCode::OK,
        "原始 body 上传应成功: {}",
        String::from_utf8_lossy(&response_bytes(upload).await)
    );
    let payload = response_json(upload).await;
    assert_eq!(payload["completed"], Value::from(true));
    assert_eq!(payload["filename"], "构建产物.txt");
    assert_eq!(payload["size"], Value::from(11));
    assert_eq!(payload["path"], "ci/构建产物.txt");

    // 内容可原样下载
    let download = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/content?path=ci/%E6%9E%84%E5%BB%BA%E4%BA%A7%E7%89%A9.txt")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(&response_bytes(download).await[..], b"hello build");

    // 2) 只给 path（最后一段当文件名），根目录
    let root_upload = app
        .request_with_cookie(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/files/upload?path=%E6%A0%B9%E7%9B%AE%E5%BD%95.txt")
                .body(Body::from("root file".as_bytes().to_vec()))
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(root_upload.status(), StatusCode::OK);
    let root_payload = response_json(root_upload).await;
    assert_eq!(root_payload["path"], "根目录.txt");

    // 3) 覆盖同名文件会生成新版本
    let second = app
        .request_with_cookie(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/files/upload?path=%E6%A0%B9%E7%9B%AE%E5%BD%95.txt&message=%E7%AC%AC%E4%BA%8C%E7%89%88")
                .body(Body::from("root file v2".as_bytes().to_vec()))
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(second.status(), StatusCode::OK);
    let history = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/history?path=%E6%A0%B9%E7%9B%AE%E5%BD%95.txt")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let messages: Vec<String> = response_json(history).await["data"]["commits"]
        .as_array()
        .expect("commits")
        .iter()
        .filter_map(|commit| commit["message"].as_str().map(str::to_string))
        .collect();
    assert!(
        messages.iter().any(|message| message.contains("第二版")),
        "覆盖上传应写入版本说明: {messages:?}"
    );

    // 4) 目标是已存在的目录 → 400（提示明确，而不是模糊的冲突）
    let directory_target = app
        .request_with_cookie(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/files/upload?path=ci")
                .body(Body::from("x".as_bytes().to_vec()))
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(directory_target.status(), StatusCode::BAD_REQUEST);
    let directory_payload = response_json(directory_target).await;
    assert!(
        directory_payload["message"]
            .as_str()
            .is_some_and(|message| message.contains("directory")),
        "应提示目标是目录: {directory_payload:?}"
    );

    // 5) URL 路径形式（curl -T 会把文件名拼到 URL 后面）
    let url_form = app
        .request_with_cookie(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/files/upload/ci/url-form.txt?message=URL%20%E5%BD%A2%E5%BC%8F")
                .body(Body::from("via url path".as_bytes().to_vec()))
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(
        url_form.status(),
        StatusCode::OK,
        "URL 路径形式应可用: {}",
        String::from_utf8_lossy(&response_bytes(url_form).await)
    );
    let url_payload = response_json(url_form).await;
    assert_eq!(url_payload["path"], "ci/url-form.txt");

    let url_download = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/content?path=ci/url-form.txt")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(url_download.status(), StatusCode::OK);
    assert_eq!(&response_bytes(url_download).await[..], b"via url path");

    let anonymous = app
        .request(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/files/upload?path=ci&filename=a.txt")
                .body(Body::from("x".as_bytes().to_vec()))
                .expect("request should build"),
        )
        .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
}

/// 访问令牌：可用来上传/下载（CLI、构建系统场景），撤销后失效，且不能自我扩权。
#[tokio::test]
async fn access_token_authenticates_api_requests() {
    use axum::http::header;

    let app = TestApp::new().await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    // 1) 创建令牌（仅会话鉴权）
    let created = app
        .json_request_with_cookie(
            Method::POST,
            "/api/tokens",
            json!({ "name": "CI 构建", "expires_in_days": 90 }),
            &admin_cookie,
        )
        .await;
    assert_eq!(created.status(), StatusCode::OK);
    let payload = response_json(created).await;
    let plaintext = payload["plaintext"]
        .as_str()
        .expect("plaintext token")
        .to_string();
    assert!(
        plaintext.starts_with("vfat_") && plaintext.len() > 40,
        "令牌应带前缀且足够长: {plaintext}"
    );
    assert_eq!(payload["token"]["name"], "CI 构建");
    assert!(payload["token"]["active"].as_bool().unwrap_or(false));
    // 列表里只有前缀，没有明文
    assert_eq!(
        payload["token"]["token_prefix"].as_str().map(str::len),
        Some("vfat_".len() + 8)
    );

    let bearer = format!("Bearer {plaintext}");

    // 2) 用令牌列目录
    let list = app
        .request(
            Request::builder()
                .uri("/api/files/tree")
                .header(header::AUTHORIZATION, &bearer)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(list.status(), StatusCode::OK, "令牌应能列目录");

    // 3) 用令牌上传（init → chunk → complete）
    let init = app
        .request(
            Request::builder()
                .method(Method::POST)
                .uri("/api/files/upload/init")
                .header(header::AUTHORIZATION, &bearer)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "path": "ci",
                        "filename": "构建产物.txt",
                        "size": 5,
                        "chunk_size": 5,
                    }))
                    .expect("json body"),
                ))
                .expect("request should build"),
        )
        .await;
    assert_eq!(
        init.status(),
        StatusCode::OK,
        "令牌应能发起上传: {}",
        String::from_utf8_lossy(&response_bytes(init).await)
    );
    let upload_id = response_json(init).await["upload_id"]
        .as_str()
        .expect("upload_id")
        .to_string();

    let chunk = app
        .request(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/files/upload/chunks/{upload_id}/0"))
                .header(header::AUTHORIZATION, &bearer)
                .body(Body::from("build".as_bytes().to_vec()))
                .expect("request should build"),
        )
        .await;
    assert_eq!(chunk.status(), StatusCode::OK, "令牌应能上传分片");

    let complete = app
        .request(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/files/upload/complete/{upload_id}"))
                .header(header::AUTHORIZATION, &bearer)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "message": "由 CI 上传" })).expect("json body"),
                ))
                .expect("request should build"),
        )
        .await;
    assert_eq!(
        complete.status(),
        StatusCode::OK,
        "令牌应能完成上传: {}",
        String::from_utf8_lossy(&response_bytes(complete).await)
    );

    // 4) 用令牌下载，内容一致
    let download = app
        .request(
            Request::builder()
                .uri(format!(
                    "/api/files/content?path={}",
                    "ci/%E6%9E%84%E5%BB%BA%E4%BA%A7%E7%89%A9.txt"
                ))
                .header(header::AUTHORIZATION, &bearer)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(download.status(), StatusCode::OK, "令牌应能下载文件");
    let body = response_bytes(download).await;
    assert_eq!(&body[..], b"build", "下载内容应与上传一致");

    // 5) 令牌出现在列表里，且记录了最近使用时间
    let list_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/tokens")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let tokens = response_json(list_response).await;
    let items = tokens.as_array().expect("token list");
    assert_eq!(items.len(), 1, "应只有一个令牌: {tokens:?}");
    assert!(
        items[0]["last_used_at"].is_string(),
        "使用后应记录 last_used_at: {tokens:?}"
    );

    // 6) 令牌不能创建令牌（防止泄露后自我扩权）
    let escalate = app
        .request(
            Request::builder()
                .method(Method::POST)
                .uri("/api/tokens")
                .header(header::AUTHORIZATION, &bearer)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "name": "不该成功" })).expect("json body"),
                ))
                .expect("request should build"),
        )
        .await;
    assert_eq!(escalate.status(), StatusCode::FORBIDDEN);

    // 7) 撤销后令牌立即失效
    let token_id = items[0]["id"].as_str().expect("token id").to_string();
    let revoked = app
        .request_with_cookie(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/tokens/{token_id}"))
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let after_revoke = app
        .request(
            Request::builder()
                .uri("/api/files/tree")
                .header(header::AUTHORIZATION, &bearer)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(
        after_revoke.status(),
        StatusCode::UNAUTHORIZED,
        "撤销后的令牌不应通过鉴权"
    );

    // 8) 非法有效期与匿名创建
    let bad_expiry = app
        .json_request_with_cookie(
            Method::POST,
            "/api/tokens",
            json!({ "name": "bad", "expires_in_days": 7 }),
            &admin_cookie,
        )
        .await;
    assert_eq!(bad_expiry.status(), StatusCode::BAD_REQUEST);

    let anonymous = app
        .json_request(Method::POST, "/api/tokens", json!({ "name": "anon" }))
        .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
}

/// 所有权转移：文件连同版本历史转给另一个用户，双方各自能看到应有的内容。
#[tokio::test]
async fn transfer_moves_entry_and_version_history_to_target_user() {
    let app = TestApp::new().await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    // admin 上传两个版本，制造历史
    app.upload_version("docs", "交接.txt", b"v1\n", "第一版")
        .await;
    app.upload_version("docs", "交接.txt", b"v2 content\n", "第二版")
        .await;

    // alice 接收
    app.register_user("receiver", "receiver@example.com", "receiver-password")
        .await;
    let alice_cookie = app.login_cookie("receiver", "receiver-password").await;
    let alice_bootstrap = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/session/bootstrap")
                .body(Body::empty())
                .expect("request should build"),
            &alice_cookie,
        )
        .await;
    let target_user_id = response_json(alice_bootstrap).await["current_user"]
        .as_str()
        .expect("target user id")
        .to_string();

    let transfer = app
        .json_request_with_cookie(
            Method::POST,
            "/api/files/transfer",
            json!({ "paths": ["docs/交接.txt"], "target_user_id": target_user_id }),
            &admin_cookie,
        )
        .await;
    assert_eq!(
        transfer.status(),
        StatusCode::OK,
        "转移应成功: {}",
        String::from_utf8_lossy(&response_bytes(transfer).await)
    );

    // 源侧：文件消失
    let source_list = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/tree/docs")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let source_payload = response_json(source_list).await;
    let source_names: Vec<&str> = source_payload
        .as_array()
        .expect("source tree")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(
        !source_names.contains(&"交接.txt"),
        "源用户不应再看到该文件: {source_names:?}"
    );

    // 目标侧：文件出现，且**版本历史完整**
    let target_list = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/tree/docs")
                .body(Body::empty())
                .expect("request should build"),
            &alice_cookie,
        )
        .await;
    let target_payload = response_json(target_list).await;
    let target_names: Vec<&str> = target_payload
        .as_array()
        .expect("target tree")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(
        target_names.contains(&"交接.txt"),
        "目标用户应看到该文件: {target_names:?}"
    );

    let history = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/history?path=docs/交接.txt")
                .body(Body::empty())
                .expect("request should build"),
            &alice_cookie,
        )
        .await;
    assert_eq!(history.status(), StatusCode::OK);
    let history_payload = response_json(history).await;
    // 历史接口返回 { success, data: { commits: [...] } }
    let messages: Vec<&str> = history_payload["data"]["commits"]
        .as_array()
        .expect("commits")
        .iter()
        .filter_map(|commit| commit["message"].as_str())
        .collect();
    assert!(
        messages.iter().any(|message| message.contains("第一版"))
            && messages.iter().any(|message| message.contains("第二版")),
        "版本历史应随所有权一起转移: {messages:?}"
    );

    // 不能转给自己
    let admin_bootstrap = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/session/bootstrap")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let admin_id = response_json(admin_bootstrap).await["current_user"]
        .as_str()
        .expect("admin id")
        .to_string();
    app.upload_version("", "自转.txt", b"x", "seed").await;
    let self_transfer = app
        .json_request_with_cookie(
            Method::POST,
            "/api/files/transfer",
            json!({ "paths": ["自转.txt"], "target_user_id": admin_id }),
            &admin_cookie,
        )
        .await;
    assert_eq!(self_transfer.status(), StatusCode::BAD_REQUEST);
}

/// 转移冲突与鉴权：目标已有同名条目时整体失败；未登录不可调用。
#[tokio::test]
async fn transfer_rejects_conflicts_and_requires_auth() {
    let app = TestApp::new().await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    app.upload_version("", "同名.txt", b"admin\n", "seed").await;

    app.register_user("owner2", "owner2@example.com", "owner2-password")
        .await;
    let other_cookie = app.login_cookie("owner2", "owner2-password").await;
    app.upload_version_with_cookie(&other_cookie, "", "同名.txt", b"other\n", "seed")
        .await;

    let bootstrap = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/session/bootstrap")
                .body(Body::empty())
                .expect("request should build"),
            &other_cookie,
        )
        .await;
    let target_id = response_json(bootstrap).await["current_user"]
        .as_str()
        .expect("user id")
        .to_string();

    let conflict = app
        .json_request_with_cookie(
            Method::POST,
            "/api/files/transfer",
            json!({ "paths": ["同名.txt"], "target_user_id": target_id }),
            &admin_cookie,
        )
        .await;
    assert_eq!(
        conflict.status(),
        StatusCode::CONFLICT,
        "目标已有同名条目应返回冲突"
    );

    let anonymous = app
        .json_request(
            Method::POST,
            "/api/files/transfer",
            json!({ "paths": ["同名.txt"], "target_user_id": target_id }),
        )
        .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    // 目标用户列表：只包含其他可用用户
    let targets = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/files/users/directory")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let target_payload = response_json(targets).await;
    let names: Vec<&str> = target_payload
        .as_array()
        .expect("targets")
        .iter()
        .filter_map(|item| item["username"].as_str())
        .collect();
    assert!(names.contains(&"owner2"), "应列出其他用户: {names:?}");
    assert!(!names.contains(&"admin"), "不应包含自己: {names:?}");
}

/// 审计日志：登录/上传/下载都会被记录，只有管理员可读，且接口不提供修改入口。
#[tokio::test]
async fn audit_log_records_key_actions_and_is_admin_only() {
    let app = TestApp::new().await;

    // 登录（管理员）→ 上传 → 下载
    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    app.upload_version("docs", "审计.txt", b"audit\n", "seed")
        .await;

    let download = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/download?path=docs/审计.txt")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(download.status(), StatusCode::OK);

    // 非管理员不可访问
    app.register_user("auditor", "viewer@example.com", "viewer-password")
        .await;
    let viewer_cookie = app.login_cookie("auditor", "viewer-password").await;
    let forbidden = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/logs")
                .body(Body::empty())
                .expect("request should build"),
            &viewer_cookie,
        )
        .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    // 管理员可读，并包含三类动作
    let list = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/logs?limit=200")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let payload = response_json(list).await;
    let items = payload["items"]
        .as_array()
        .expect("audit items should be an array");
    let actions: Vec<&str> = items
        .iter()
        .filter_map(|item| item["action"].as_str())
        .collect();
    assert!(
        actions.contains(&"login.success"),
        "应记录登录成功: {actions:?}"
    );
    assert!(actions.contains(&"file.upload"), "应记录上传: {actions:?}");
    assert!(
        actions.contains(&"file.download"),
        "应记录下载: {actions:?}"
    );

    let download_entry = items
        .iter()
        .find(|item| item["action"] == "file.download")
        .expect("download entry should exist");
    assert_eq!(download_entry["target"], "docs/审计.txt");
    assert_eq!(download_entry["result"], "success");
    assert_eq!(download_entry["username"], "admin");

    // 失败登录同样入库
    let failed = app
        .json_request(
            Method::POST,
            "/api/auth/login",
            json!({ "username_or_email": "admin", "password": "wrong-password" }),
        )
        .await;
    assert_eq!(failed.status(), StatusCode::UNAUTHORIZED);

    let after = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/logs?result=failure")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let failed_payload = response_json(after).await;
    let failed_items = failed_payload["items"]
        .as_array()
        .expect("audit items should be an array");
    assert_eq!(
        failed_items.len(),
        1,
        "应只有一条失败记录: {failed_payload:?}"
    );
    assert_eq!(failed_items[0]["action"], "login.failure");
    assert_eq!(failed_items[0]["username"], "admin");

    // 动作列表接口用于前端筛选
    let actions_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/actions")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let action_names = response_json(actions_response).await;
    let action_names = action_names.as_array().expect("actions should be an array");
    assert!(
        action_names.iter().any(|value| value == "file.upload"),
        "动作列表应包含 file.upload: {action_names:?}"
    );

    // 只读接口：不存在删除/修改入口
    let delete_attempt = app
        .request_with_cookie(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/audit/logs")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert!(
        delete_attempt.status() == StatusCode::METHOD_NOT_ALLOWED
            || delete_attempt.status() == StatusCode::NOT_FOUND,
        "审计日志不应提供删除入口，实际状态: {}",
        delete_attempt.status()
    );
}

/// 审计概览：总量、失败数与 Top 用户/动作，并尊重筛选条件。
#[tokio::test]
async fn audit_summary_reports_totals_and_top_keys() {
    let app = TestApp::new().await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    // 再制造一次失败登录（匿名）
    app.json_request(
        Method::POST,
        "/api/auth/login",
        json!({ "username_or_email": "admin", "password": "wrong-password" }),
    )
    .await;

    let response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/summary")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    assert_eq!(
        payload["failures"],
        Value::from(1),
        "应统计出一条失败: {payload:?}"
    );
    assert!(
        payload["total"].as_u64().unwrap_or(0) >= 2,
        "应包含登录成功与失败: {payload:?}"
    );
    let users = payload["users"]
        .as_array()
        .expect("users should be an array");
    assert!(
        users.iter().any(|item| item["key"] == "admin"),
        "Top 用户应包含 admin: {payload:?}"
    );
    let actions = payload["actions"]
        .as_array()
        .expect("actions should be an array");
    assert!(
        actions.iter().any(|item| item["key"] == "login.failure"),
        "Top 动作应包含 login.failure: {payload:?}"
    );

    // 只看失败：总量与 Top 动作随之变化
    let failures = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/summary?result=failure")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let failure_payload = response_json(failures).await;
    assert_eq!(failure_payload["total"], Value::from(1));
    let failure_actions = failure_payload["actions"].as_array().unwrap();
    assert_eq!(failure_actions.len(), 1);
    assert_eq!(failure_actions[0]["key"], "login.failure");

    // 非管理员不可访问
    app.register_user("sumviewer", "viewer3@example.com", "viewer-password")
        .await;
    let viewer_cookie = app.login_cookie("sumviewer", "viewer-password").await;
    let forbidden = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/summary")
                .body(Body::empty())
                .expect("request should build"),
            &viewer_cookie,
        )
        .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

/// 审计日志导出：只读 CSV，尊重筛选，且导出动作本身会被审计。
#[tokio::test]
async fn audit_logs_csv_export_respects_filters_and_is_audited() {
    let app = TestApp::new().await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    let export = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/logs.csv?action=login.success")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(export.status(), StatusCode::OK);
    assert_eq!(
        export
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/csv; charset=utf-8")
    );
    let disposition = export
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        disposition.contains("attachment") && disposition.contains(".csv"),
        "应作为附件下载: {disposition}"
    );

    let body =
        String::from_utf8(response_bytes(export).await.to_vec()).expect("csv should be utf-8");
    let lines: Vec<&str> = body.lines().collect();
    // 前几行是汇总区块（总记录/失败/Top 用户/Top 动作），随后才是数据表头
    assert!(
        body.contains("汇总") && body.contains("Top 用户") && body.contains("Top 动作"),
        "CSV 应带汇总区块: {body:?}"
    );
    assert!(lines[0].contains("汇总"), "汇总区块应在最前面: {body:?}");
    let header_index = lines
        .iter()
        .position(|line| line.contains("时间") && line.contains("设备"))
        .expect("应包含数据表头");
    let data_lines: Vec<&&str> = lines[header_index + 1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    // 仅筛选 login.success：只有 1 条数据行
    assert_eq!(data_lines.len(), 1, "CSV 应只包含筛选后的记录: {body:?}");
    assert!(data_lines[0].contains("login.success"));
    assert!(data_lines[0].contains("admin"));

    // 导出本身写入审计（audit.export）
    let list = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/logs?action=audit.export")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    let payload = response_json(list).await;
    let items = payload["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "应记录一次导出: {payload:?}");
    assert!(
        items[0]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("动作=login.success")),
        "导出记录应包含筛选条件: {payload:?}"
    );

    // 非管理员不可导出
    app.register_user("auditor2", "auditor2@example.com", "viewer-password")
        .await;
    let viewer_cookie = app.login_cookie("auditor2", "viewer-password").await;
    let forbidden = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/audit/logs.csv")
                .body(Body::empty())
                .expect("request should build"),
            &viewer_cookie,
        )
        .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

/// 分享管理列表：必须带上被分享条目的名称、路径与类型（前端要展示文件名而不是 UUID）。
#[tokio::test]
async fn share_list_includes_entry_metadata() {
    let app = TestApp::new().await;
    app.upload_version("docs", "季报.txt", b"report\n", "seed")
        .await;
    app.json_request_as_admin(
        Method::POST,
        "/api/files/directories",
        json!({ "path": "docs/归档" }),
    )
    .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    for path in ["docs/季报.txt", "docs/归档"] {
        let response = app
            .json_request_with_cookie(
                Method::POST,
                "/api/share/shares",
                json!({ "path": path }),
                &admin_cookie,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let list = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/share/shares")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let payload = response_json(list).await;
    let shares = payload.as_array().expect("share list should be an array");
    assert_eq!(shares.len(), 2, "两个分享都应返回: {payload:?}");

    let by_name = |name: &str| {
        shares
            .iter()
            .find(|item| item["entry_name"].as_str() == Some(name))
            .cloned()
            .unwrap_or_else(|| panic!("缺少 {name} 的分享: {payload:?}"))
    };

    let file_share = by_name("季报.txt");
    assert_eq!(file_share["entry_path"], Value::from("docs/季报.txt"));
    assert_eq!(file_share["entry_kind"], Value::from("file"));
    assert!(file_share["code"].as_str().is_some_and(|v| !v.is_empty()));

    let dir_share = by_name("归档");
    assert_eq!(dir_share["entry_path"], Value::from("docs/归档"));
    assert_eq!(dir_share["entry_kind"], Value::from("directory"));
}

#[tokio::test]
async fn share_download_supports_files_and_directories() {
    let app = TestApp::new().await;

    let create_dir = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "docs" }),
        )
        .await;
    assert_eq!(create_dir.status(), StatusCode::OK);

    app.upload_version("docs", "share.txt", b"shared file\n", "share source")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    let file_share = app
        .json_request_with_cookie(
            Method::POST,
            "/api/share/shares",
            json!({ "path": "docs/share.txt" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(file_share.status(), StatusCode::OK);
    let file_code = response_json(file_share).await["code"]
        .as_str()
        .expect("file share code should be present")
        .to_string();

    let file_download = app
        .request(
            Request::builder()
                .uri(format!("/s/{}", file_code))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(file_download.status(), StatusCode::OK);
    assert_eq!(
        response_bytes(file_download).await.as_ref(),
        b"shared file\n"
    );

    let directory_share = app
        .json_request_with_cookie(
            Method::POST,
            "/api/share/shares",
            json!({ "path": "docs" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(directory_share.status(), StatusCode::OK);
    let directory_code = response_json(directory_share).await["code"]
        .as_str()
        .expect("directory share code should be present")
        .to_string();

    let directory_download = app
        .request(
            Request::builder()
                .uri(format!("/s/{}", directory_code))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(directory_download.status(), StatusCode::OK);
    assert_eq!(
        directory_download.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/zip"))
    );

    let zip_bytes = response_bytes(directory_download).await;
    let cursor = Cursor::new(zip_bytes.to_vec());
    let mut archive = zip::ZipArchive::new(cursor).expect("zip archive should open");
    let mut file = archive
        .by_name("docs/share.txt")
        .expect("shared directory archive should contain file");
    let mut extracted = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut extracted).expect("zip entry should read");
    assert_eq!(extracted, b"shared file\n");
}

#[tokio::test]
async fn share_download_supports_unicode_filenames() {
    let app = TestApp::new().await;

    let create_dir = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "docs" }),
        )
        .await;
    assert_eq!(create_dir.status(), StatusCode::OK);

    app.upload_version("docs", "中文 文件.txt", b"shared utf8\n", "share source")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let share = app
        .json_request_with_cookie(
            Method::POST,
            "/api/share/shares",
            json!({ "path": "docs/中文 文件.txt" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(share.status(), StatusCode::OK);

    let code = response_json(share).await["code"]
        .as_str()
        .expect("share code should be present")
        .to_string();

    let download = app
        .request(
            Request::builder()
                .uri(format!("/s/{}", code))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(download.status(), StatusCode::OK);

    let disposition = download
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("content disposition should be present");
    assert!(disposition.contains("filename*=UTF-8''"));
    assert!(disposition.contains("%E4%B8%AD%E6%96%87%20%E6%96%87%E4%BB%B6.txt"));
    assert_eq!(response_bytes(download).await.as_ref(), b"shared utf8\n");
}

#[tokio::test]
async fn share_download_uses_owner_namespace_in_multi_user_mode() {
    let app = TestApp::new().await;

    app.register_user("member", "member@example.com", "member-password")
        .await;
    let member_cookie = app.login_cookie("member", "member-password").await;

    app.upload_version_with_cookie(
        &member_cookie,
        "docs",
        "share.txt",
        b"member share\n",
        "member share upload",
    )
    .await;

    let share = app
        .json_request_with_cookie(
            Method::POST,
            "/api/share/shares",
            json!({ "path": "docs/share.txt" }),
            &member_cookie,
        )
        .await;
    assert_eq!(share.status(), StatusCode::OK);
    let code = response_json(share).await["code"]
        .as_str()
        .expect("share code should be present")
        .to_string();

    let download = app
        .request(
            Request::builder()
                .uri(format!("/s/{}", code))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(response_bytes(download).await.as_ref(), b"member share\n");
}

#[tokio::test]
async fn session_bootstrap_returns_distinct_active_workspace_per_user() {
    let app = TestApp::new().await;

    app.register_user("member", "member@example.com", "member-password")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let member_cookie = app.login_cookie("member", "member-password").await;

    let admin_bootstrap = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/session/bootstrap")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(admin_bootstrap.status(), StatusCode::OK);
    let admin_payload = response_json(admin_bootstrap).await;
    let admin_workspace = admin_payload["active_workspace"]
        .as_str()
        .expect("admin workspace should be present")
        .to_string();

    let member_bootstrap = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/session/bootstrap")
                .body(Body::empty())
                .expect("request should build"),
            &member_cookie,
        )
        .await;
    assert_eq!(member_bootstrap.status(), StatusCode::OK);
    let member_payload = response_json(member_bootstrap).await;
    let member_workspace = member_payload["active_workspace"]
        .as_str()
        .expect("member workspace should be present")
        .to_string();

    assert_ne!(admin_workspace, member_workspace);
}

#[tokio::test]
async fn register_respects_allow_register_config() {
    let app = TestApp::new_with_allow_register(false).await;

    let response = app
        .json_request(
            Method::POST,
            "/api/auth/register",
            json!({
                "username": "blocked",
                "email": "blocked@example.com",
                "password": "blocked-password",
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn register_allows_missing_email() {
    let app = TestApp::new().await;

    app.register_user_without_email("missingemail", "missing-email-password")
        .await;

    let cookie = app
        .login_cookie("missingemail", "missing-email-password")
        .await;
    let me_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/auth/me")
                .body(Body::empty())
                .expect("request should build"),
            &cookie,
        )
        .await;

    assert_eq!(me_response.status(), StatusCode::OK);
    let me_payload = response_json(me_response).await;
    assert_eq!(
        me_payload["data"]["user"]["username"],
        Value::String("missingemail".to_string())
    );
    assert!(me_payload["data"]["user"]["email"].is_null());
}

#[tokio::test]
async fn share_and_history_routes_respect_feature_flags() {
    let mut features = default_features();
    features.share_enabled = false;
    features.history_enabled = false;
    let app = TestApp::new_with_features(features).await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    let share = app
        .json_request_with_cookie(
            Method::POST,
            "/api/share/shares",
            json!({ "path": "docs/missing.txt" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(share.status(), StatusCode::FORBIDDEN);

    let history = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/history?path=docs/missing.txt")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(history.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn content_search_respects_feature_flag() {
    let app = TestApp::new().await;

    let search = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/search?q=hello&search_content=true")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(search.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn file_search_finds_dotted_filename() {
    let app = TestApp::new().await;

    let packages_root = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "packages" }),
        )
        .await;
    assert_eq!(packages_root.status(), StatusCode::OK);

    let packages_dir = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/directories",
            json!({ "path": "packages/2.4-dir" }),
        )
        .await;
    assert_eq!(packages_dir.status(), StatusCode::OK);

    app.upload_version(
        "packages",
        "2.4.3a0.tgz",
        b"archive payload",
        "seed dotted archive",
    )
    .await;
    app.upload_version("docs", "2.4.9.txt", b"side file", "seed side file")
        .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/search?q=2.4&search_files=true&path=packages&type=file&limit=500")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload["has_more"], Value::Bool(false));
    assert_eq!(payload["limit"], Value::from(500));
    let results = payload["items"]
        .as_array()
        .expect("search response should carry an items array");

    assert_eq!(results.len(), 1);
    assert!(results.iter().any(|item| {
        item["entry"]["path"] == Value::String("packages/2.4.3a0.tgz".to_string())
            && item["entry"]["name"] == Value::String("2.4.3a0.tgz".to_string())
    }));
}

/// 健康检查暴露缩略图计数：先打一次不支持格式的请求，计数应随之上浮。
#[tokio::test]
async fn health_reports_thumbnail_counters() {
    let app = TestApp::new().await;

    let before = app
        .request(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(before.status(), StatusCode::OK);
    let before_payload = response_json(before).await;
    let unsupported_before = before_payload["thumbnail"]["unsupported"]
        .as_u64()
        .expect("thumbnail counters should be exposed");
    assert!(
        before_payload["thumbnail"]["generated"].is_u64(),
        "generated counter should be present"
    );

    // 上传一个文本文件并请求缩略图：格式不支持 → unsupported 计数 +1
    app.upload_version("", "notes.txt", b"plain text", "seed text")
        .await;
    let thumbnail = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/thumbnail?path=notes.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(thumbnail.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let after = app
        .request(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let after_payload = response_json(after).await;
    // 计数是进程级的，测试并行执行时其他用例也会累加，因此用「增量下界」断言
    let unsupported_after = after_payload["thumbnail"]["unsupported"]
        .as_u64()
        .expect("unsupported counter");
    assert!(
        unsupported_after > unsupported_before,
        "unsupported counter should grow: {unsupported_before} -> {unsupported_after}"
    );
    assert!(
        after_payload["thumbnail"]["failed"].as_u64().is_some(),
        "failed counter should be present"
    );
}

#[tokio::test]
async fn file_search_pages_results_with_has_more() {
    let app = TestApp::new().await;

    // 5 个同名片段、不同序号的条目
    for index in 0..5 {
        app.upload_version(
            "",
            &format!("report-{index}.txt"),
            b"paged payload",
            "seed paged search",
        )
        .await;
    }

    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u32;
    let mut pages = 0;

    loop {
        let response = app
            .request_as_admin(
                Request::builder()
                    .uri(format!(
                        "/api/files/search?q=report&search_files=true&limit=2&offset={offset}"
                    ))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);

        let payload = response_json(response).await;
        assert_eq!(payload["limit"], Value::from(2));
        assert_eq!(payload["offset"], Value::from(offset));

        let items = payload["items"]
            .as_array()
            .expect("search response should carry an items array")
            .clone();
        assert!(
            items.len() <= 2,
            "page must respect the requested limit, got {}",
            items.len()
        );

        for item in &items {
            seen.push(
                item["entry"]["path"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
        }

        let has_more = payload["has_more"]
            .as_bool()
            .expect("has_more should exist");
        pages += 1;
        if !has_more {
            break;
        }
        assert!(pages < 5, "paging should terminate");
        offset += 2;
    }

    assert_eq!(seen.len(), 5, "所有命中都应通过翻页返回: {seen:?}");
    let mut sorted = seen.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 5, "翻页不应重复返回同一条目: {seen:?}");
    assert_eq!(pages, 3, "5 条命中按每页 2 条应为 3 页");
}

/// 同时开启文件名与内容搜索时，翻页必须是同一份「按得分排序」结果的连续切片。
///
/// 回归用例：此前 `LIMIT/OFFSET` 由仓储层按 `created_at` 分别执行，两路结果合并后
/// 再按得分排序，导致文件名命中（得分更高）每一页都会把内容命中挤出窗口，内容命中
/// 在第二页之后完全取不到。
#[tokio::test]
async fn paged_search_keeps_content_matches_across_pages() {
    let mut features = default_features();
    features.search_content = true;
    let app = TestApp::new_with_features(features).await;

    // 文件名命中：名字里带 needle
    app.upload_version("", "needle-name.txt", b"plain body", "seed name hit")
        .await;
    // 内容命中：名字不含 needle，正文包含
    app.upload_version(
        "",
        "body-only.txt",
        b"the needle lives in the body",
        "seed content hit",
    )
    .await;
    // 另一条内容命中，用于验证第二页
    app.upload_version(
        "",
        "second-body.txt",
        b"another needle here",
        "seed content hit 2",
    )
    .await;

    let mut seen: Vec<(String, f64)> = Vec::new();
    let mut offset = 0u32;

    loop {
        let response = app
            .request_as_admin(
                Request::builder()
                    .uri(format!(
                        "/api/files/search?q=needle&search_files=true&search_content=true&limit=1&offset={offset}"
                    ))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);

        let payload = response_json(response).await;
        let items = payload["items"].as_array().expect("items array").clone();
        for item in &items {
            seen.push((
                item["entry"]["path"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                item["score"].as_f64().unwrap_or_default(),
            ));
        }

        if !payload["has_more"].as_bool().unwrap_or(false) {
            break;
        }
        offset += 1;
        assert!(offset < 10, "paging should terminate");
    }

    let paths: Vec<&str> = seen.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(seen.len(), 3, "文件名与内容命中都应通过翻页返回: {paths:?}");
    let mut unique = paths.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 3, "翻页不应重复: {paths:?}");
    // 得分更高的文件名命中必须排在内容命中之前
    assert_eq!(
        paths[0], "needle-name.txt",
        "文件名命中应排在最前: {paths:?}"
    );
    assert!(
        seen.iter()
            .all(|(path, score)| path == "needle-name.txt" || *score < 1.0),
        "内容命中的得分应低于文件名命中: {seen:?}"
    );
}

/// 概览按类型聚合占用：文档 / 图片分别统计字节数，其它类型归入 other。
#[tokio::test]
async fn overview_reports_storage_usage_by_category() {
    let app = TestApp::new().await;

    // doc: text/plain、图片: png、未知类型: .bin（无 MIME 推断 → other）
    app.upload_version("", "notes.txt", b"0123456789", "seed")
        .await;
    app.upload_version("", "shot.png", &[0u8; 30], "seed").await;
    app.upload_version("", "blob.bin", &[0u8; 5], "seed").await;
    app.upload_version("", "second.md", b"abcdefgh", "seed")
        .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/overview")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload["total_bytes"], Value::from(53));

    let categories = payload["categories"]
        .as_array()
        .expect("categories should be an array")
        .clone();
    let find = |name: &str| {
        categories
            .iter()
            .find(|item| item["category"].as_str() == Some(name))
            .cloned()
    };

    let document = find("document").expect("document category should exist");
    assert_eq!(document["bytes"], Value::from(18), "txt + md");
    assert_eq!(document["file_count"], Value::from(2));

    let image = find("image").expect("image category should exist");
    assert_eq!(image["bytes"], Value::from(30));
    assert_eq!(image["file_count"], Value::from(1));

    let other = find("other").expect("other category should exist");
    assert_eq!(other["bytes"], Value::from(5));

    // 分类按字节数倒序
    let order: Vec<&str> = categories
        .iter()
        .map(|item| item["category"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        order.first(),
        Some(&"image"),
        "占用最大的分类应排在最前: {order:?}"
    );
}

/// 空命名空间的概览不应报错，分类为空数组。
#[tokio::test]
async fn overview_on_empty_namespace_has_no_categories() {
    let app = TestApp::new().await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/overview")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let payload = response_json(response).await;

    assert_eq!(payload["file_count"], Value::from(0));
    assert_eq!(
        payload["categories"],
        Value::Array(Vec::new()),
        "空命名空间应返回空数组而不是缺字段"
    );
}

/// FTP 连接信息：需要登录，返回配置但不含任何密钥。
#[tokio::test]
async fn ftp_info_reports_configuration_for_authenticated_users() {
    let app = TestApp::new().await;

    let anonymous = app
        .request(
            Request::builder()
                .uri("/api/files/ftp-info")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(
        anonymous.status(),
        StatusCode::UNAUTHORIZED,
        "未登录不应泄露 FTP 连接信息"
    );

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/ftp-info")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload["enabled"], Value::from(true), "FTP 默认开启");
    assert_eq!(payload["port"], Value::from(2121));
    assert_eq!(payload["passive_ports"]["start"], Value::from(50000));
    assert_eq!(payload["passive_ports"]["end"], Value::from(50100));
    assert_eq!(payload["tls"]["enabled"], Value::from(false));
    assert!(
        payload["example_command"].is_string(),
        "开启时应给出连接示例"
    );
    // 不泄露口令
    assert!(
        !serde_json::to_string(&payload)
            .unwrap_or_default()
            .to_lowercase()
            .contains("password"),
        "响应不应包含任何口令字段"
    );
}

/// 健康检查包含 FTP 计数块（默认全 0）。
#[tokio::test]
async fn health_reports_ftp_counters() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload["ftp"]["active_sessions"], Value::from(0));
    assert_eq!(payload["ftp"]["files_uploaded"], Value::from(0));
    assert_eq!(payload["ftp"]["bytes_uploaded"], Value::from(0));
}

/// 目录列表只返回**直接子条目**：嵌套目录的后代不能出现在父目录的列表里。
///
/// 回归用例：round 60 把分页下沉到 SQL 时用 `path >= 'a/' AND path < 'a0'`
/// 做范围匹配，这会把整棵子树（如 `a/docs/x`）也算作 `a` 的子条目，
/// 导致列表多出孙子条目、目录树出现重复行。
#[tokio::test]
async fn directory_listing_returns_only_direct_children() {
    let app = TestApp::new().await;

    for path in ["a", "a/docs", "a/docs/x", "b", "b/docs"] {
        let created = app
            .request_as_admin(
                Request::builder()
                    .method("POST")
                    .uri("/api/files/directories")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!("{{\"path\":\"{path}\"}}")))
                    .expect("request should build"),
            )
            .await;
        assert!(
            created.status().is_success(),
            "create {path}: {}",
            created.status()
        );
    }
    app.upload_version("a", "top.txt", b"top", "seed").await;
    app.upload_version("a/docs", "deep.txt", b"deep", "seed")
        .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list/a?limit=50")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let mut paths: Vec<String> = payload["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["path"].as_str().unwrap_or_default().to_string())
        .collect();
    paths.sort();

    assert_eq!(
        paths,
        vec!["a/docs".to_string(), "a/top.txt".to_string(),],
        "只应返回直接子条目，不能包含 a/docs/x 或 a/docs/deep.txt"
    );
    assert_eq!(payload["total"], Value::from(2), "total 也只统计直接子条目");

    // 再深一层：a/docs 只包含它自己的直接子条目
    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list/a/docs?limit=50")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let payload = response_json(response).await;
    let mut paths: Vec<String> = payload["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["path"].as_str().unwrap_or_default().to_string())
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec!["a/docs/deep.txt".to_string(), "a/docs/x".to_string()],
    );
}

/// 目录分页在 SQL 侧完成：目录优先、页间连续、total/has_more 正确。
#[tokio::test]
async fn directory_listing_pages_in_sql_with_directory_first_order() {
    let app = TestApp::new().await;
    app.upload_version("", "b.txt", b"b", "seed").await;
    app.upload_version("", "a.txt", b"a", "seed").await;
    app.upload_version("", "c.txt", b"c", "seed").await;
    // 先建目录（空内容的"上传"会被分片校验拒绝）
    let created = app
        .request_as_admin(
            Request::builder()
                .method("POST")
                .uri("/api/files/directories")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"path\":\"zdir\"}"))
                .expect("request should build"),
        )
        .await;
    assert!(
        created.status().is_success(),
        "create dir: {}",
        created.status()
    );

    // 第一页：目录优先，其余按名称升序
    let first = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list?limit=2&offset=0")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(first.status(), StatusCode::OK);
    let payload = response_json(first).await;
    assert_eq!(payload["total"], Value::from(4));
    assert_eq!(payload["limit"], Value::from(2));
    assert_eq!(payload["offset"], Value::from(0));
    assert_eq!(payload["has_more"], Value::Bool(true));
    let names: Vec<String> = payload["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(names, vec!["zdir", "a.txt"]);

    // 第二页与第一页连续
    let second = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list?limit=2&offset=2")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let payload = response_json(second).await;
    assert_eq!(payload["has_more"], Value::Bool(false));
    let names: Vec<String> = payload["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(names, vec!["b.txt", "c.txt"]);

    // 越界偏移返回空页
    let beyond = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list?limit=2&offset=50")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let payload = response_json(beyond).await;
    assert_eq!(payload["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(payload["has_more"], Value::Bool(false));

    // 子目录同样走分页路径
    let sub = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list/zdir?limit=5&offset=0")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(sub.status(), StatusCode::OK);
    let payload = response_json(sub).await;
    assert_eq!(payload["total"], Value::from(0));
}

/// 请求体本身非法时，也要返回统一的错误信封（而不是 axum 的 422 纯文本）。
#[tokio::test]
async fn malformed_json_body_uses_the_standard_error_envelope() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{not-json"))
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(payload["code"], Value::from("VALIDATION_FAILED"));
    assert_eq!(payload["details"]["field"], Value::from("body"));
    assert!(
        payload["details"]["reason"].is_string(),
        "应给出结构化原因: {}",
        payload["details"]
    );
    assert!(payload["request_id"].is_string(), "统一信封应带 request_id");
}

/// 校验失败与超限都返回结构化的 `details`，便于客户端本地化展示。
#[tokio::test]
async fn validation_and_size_errors_carry_structured_details() {
    let app = TestApp::new().await;

    // 非法查询参数（handler 内的 Validation）：details 应带结构化原因
    let invalid = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/search?q=report&type=bogus")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(invalid).await;
    assert_eq!(payload["code"], Value::from("VALIDATION_FAILED"));
    assert!(
        payload["details"]["reason"].is_string(),
        "校验失败应给出结构化原因: {}",
        payload["details"]
    );

    // 路径冲突：details.path（round 49 已加，这里一并回归）
    app.upload_version("", "exists.txt", b"x", "seed").await;
    let conflict = app
        .request_as_admin(
            Request::builder()
                .method("POST")
                .uri("/api/files/move")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"from\":\"exists.txt\",\"to\":\"exists.txt\"}",
                ))
                .expect("request should build"),
        )
        .await;
    let payload = response_json(conflict).await;
    // 同一个路径的移动会走 Conflict 分支（带 message），details 仍是结构化的
    assert!(
        matches!(
            payload["code"].as_str(),
            Some("PATH_CONFLICT") | Some("CONFLICT")
        ),
        "unexpected code: {}",
        payload["code"]
    );
    assert!(payload["details"].is_object(), "冲突应带结构化 details");
}

/// 收藏：添加、列出、取消收藏，并且路径不存在时报 404。
#[tokio::test]
async fn favorites_can_be_added_listed_and_removed() {
    let app = TestApp::new().await;
    app.upload_version("", "keep.txt", b"content", "seed").await;
    app.upload_version("docs", "doc.md", b"doc", "seed").await;

    // 初始为空
    let empty = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/favorites")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(empty.status(), StatusCode::OK);
    let payload = response_json(empty).await;
    assert_eq!(payload["items"].as_array().map(Vec::len), Some(0));

    // 添加两个（其中一个为目录）
    for path in ["keep.txt", "docs"] {
        let response = app
            .request_as_admin(
                Request::builder()
                    .method("POST")
                    .uri("/api/files/favorites")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!("{{\"path\":\"{path}\"}}")))
                    .expect("request should build"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED, "add {path}");
    }

    // 重复添加是幂等的
    let again = app
        .request_as_admin(
            Request::builder()
                .method("POST")
                .uri("/api/files/favorites")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"path\":\"keep.txt\"}"))
                .expect("request should build"),
        )
        .await;
    assert_eq!(again.status(), StatusCode::CREATED);
    let payload = response_json(again).await;
    assert_eq!(payload["items"].as_array().map(Vec::len), Some(2));

    // 列表包含名称与类型
    let listed = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/favorites")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let payload = response_json(listed).await;
    let items = payload["items"].as_array().expect("items array");
    let mut pairs: Vec<(String, String)> = items
        .iter()
        .map(|item| {
            (
                item["path"].as_str().unwrap_or_default().to_string(),
                item["kind"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            ("docs".to_string(), "directory".to_string()),
            ("keep.txt".to_string(), "file".to_string()),
        ]
    );

    // 取消收藏
    let removed = app
        .request_as_admin(
            Request::builder()
                .method("DELETE")
                .uri("/api/files/favorites?path=keep.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(removed.status(), StatusCode::OK);
    let payload = response_json(removed).await;
    assert_eq!(payload["items"].as_array().map(Vec::len), Some(1));

    // 不存在的路径 → 404
    let missing = app
        .request_as_admin(
            Request::builder()
                .method("POST")
                .uri("/api/files/favorites")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"path\":\"nope.txt\"}"))
                .expect("request should build"),
        )
        .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    // 未登录 → 401
    let anonymous = app
        .request(
            Request::builder()
                .uri("/api/files/favorites")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
}

/// 删除条目后收藏应随外键级联清除。
#[tokio::test]
async fn favorites_are_cascaded_when_the_entry_is_deleted() {
    let app = TestApp::new().await;
    app.upload_version("", "temp.txt", b"x", "seed").await;
    app.upload_version("", "keep.txt", b"y", "seed").await;

    for path in ["temp.txt", "keep.txt"] {
        let response = app
            .request_as_admin(
                Request::builder()
                    .method("POST")
                    .uri("/api/files/favorites")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!("{{\"path\":\"{path}\"}}")))
                    .expect("request should build"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let deleted = app
        .request_as_admin(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/files?path=temp.txt&message=delete")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert!(
        deleted.status().is_success(),
        "delete should succeed: {}",
        deleted.status()
    );

    let listed = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/favorites")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let payload = response_json(listed).await;
    let items = payload["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "被删除条目的收藏应级联清除");
    assert_eq!(items[0]["path"], Value::from("keep.txt"));
}

/// 侧栏聚合：统计文件/目录数量与总字节数，并按时间倒序返回最近文件。
#[tokio::test]
async fn overview_reports_namespace_stats_and_recent_files() {
    let app = TestApp::new().await;

    app.upload_version("", "first.txt", b"12345", "seed first")
        .await;
    app.upload_version("docs", "second.md", b"1234567890", "seed second")
        .await;
    // 覆盖同一条目：大小应只统计当前版本
    app.upload_version("", "first.txt", b"123", "seed first again")
        .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/overview")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload["file_count"], Value::from(2));
    assert_eq!(payload["directory_count"], Value::from(1));
    assert_eq!(
        payload["total_bytes"],
        Value::from(13),
        "只统计当前版本：3 + 10"
    );

    let recent = payload["recent_files"]
        .as_array()
        .expect("recent_files array");
    assert_eq!(recent.len(), 2);
    // 最近更新的是 first.txt 的新版本
    assert_eq!(recent[0]["path"], Value::from("first.txt"));
    assert_eq!(recent[0]["name"], Value::from("first.txt"));
    assert_eq!(recent[0]["size_bytes"], Value::from(3));
    assert_eq!(recent[1]["path"], Value::from("docs/second.md"));
    assert_eq!(recent[1]["name"], Value::from("second.md"));
    assert!(
        recent[0]["updated_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "最近文件应带更新时间"
    );
}

/// 侧栏聚合需要登录态，未认证时应拒绝。
#[tokio::test]
async fn overview_requires_authentication() {
    let app = TestApp::new().await;

    let response = app
        .request(
            Request::builder()
                .uri("/api/files/overview")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// 默认配置（未开启 AVIF）下，缩略图一律 JPEG，并声明 `Vary: accept`。
///
/// AVIF 编码开销远高于 JPEG，默认关闭；协商与编码逻辑由 thumbnail.rs 的单测覆盖，
/// 开启后的端到端行为在发布前用 `VFILES_THUMBNAIL_AVIF=true` 冒烟验证。
#[tokio::test]
async fn thumbnail_returns_jpeg_and_varies_on_accept() {
    let app = TestApp::new().await;

    let source = sample_png();
    app.upload_version("", "photo.png", &source, "seed image")
        .await;

    for accept in [
        "image/avif,image/webp,image/apng,image/*,*/*;q=0.8",
        "image/webp,image/png",
        "image/jpeg,image/png",
    ] {
        let response = app
            .request_as_admin(
                Request::builder()
                    .uri("/api/files/thumbnail?path=photo.png&size=64")
                    .header(header::ACCEPT, accept)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK, "accept: {accept}");

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let vary = response
            .headers()
            .get(header::VARY)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body should collect")
            .to_bytes();

        assert_eq!(content_type, "image/jpeg", "accept: {accept}");
        assert_eq!(vary, "accept", "响应必须声明 Vary，避免缓存串味");
        assert_eq!(&body[..2], &[0xFF, 0xD8], "应返回 JPEG 魔数");
    }
}

/// 一张 8×8 的 PNG 源图，供缩略图相关用例复用。
fn sample_png() -> Vec<u8> {
    use image::{Rgba, RgbaImage};

    let mut image = RgbaImage::new(8, 8);
    for pixel in image.pixels_mut() {
        *pixel = Rgba([10, 120, 200, 255]);
    }
    let mut buffer = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, image::ImageFormat::Png)
        .expect("sample png");
    buffer.into_inner()
}

/// 偏移量超过结果总数时应返回空页且不再声明还有下一页。
#[tokio::test]
async fn search_offset_beyond_results_returns_empty_page() {
    let app = TestApp::new().await;
    app.upload_version("", "needle.txt", b"body", "seed").await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/search?q=needle&search_files=true&limit=5&offset=50")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(payload["has_more"], Value::Bool(false));
    assert_eq!(payload["offset"], Value::from(50));
}

#[tokio::test]
async fn content_search_returns_line_matches_when_feature_enabled() {
    let mut features = default_features();
    features.search_content = true;
    let app = TestApp::new_with_features(features).await;

    app.upload_version(
        "docs",
        "notes.txt",
        b"alpha\nhello streaming search\nomega\n",
        "seed search content",
    )
    .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/search?q=streaming&search_content=true")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let results = payload["items"]
        .as_array()
        .expect("search response should carry an items array");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0]["entry"]["path"],
        Value::String("docs/notes.txt".to_string())
    );
    assert_eq!(
        results[0]["matches"][0]["match_type"],
        Value::String("content".to_string())
    );
    assert_eq!(results[0]["matches"][0]["line_number"], Value::from(2));
    assert_eq!(
        results[0]["matches"][0]["context"],
        Value::String("hello streaming search".to_string())
    );
}

#[tokio::test]
async fn incomplete_chunked_upload_returns_conflict_on_completion() {
    let app = TestApp::new().await;

    let init_response = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/upload/init",
            json!({
                "path": "docs",
                "filename": "partial.bin",
                "size": 8,
                "chunk_size": 4,
            }),
        )
        .await;
    assert_eq!(init_response.status(), StatusCode::OK);
    let init_payload = response_json(init_response).await;
    let upload_id = init_payload["upload_id"]
        .as_str()
        .expect("upload_id should be present");

    let chunk_response = app
        .bytes_request_as_admin(
            Method::PUT,
            &format!("/api/files/upload/chunks/{}/0", upload_id),
            b"part".to_vec(),
        )
        .await;
    assert_eq!(chunk_response.status(), StatusCode::OK);

    let complete_response = app
        .json_request_as_admin(
            Method::POST,
            &format!("/api/files/upload/complete/{}", upload_id),
            json!({ "message": "should fail" }),
        )
        .await;
    assert_eq!(complete_response.status(), StatusCode::CONFLICT);

    let payload = response_json(complete_response).await;
    assert_eq!(
        payload["code"],
        Value::String("UPLOAD_CONFLICT".to_string())
    );
}

#[tokio::test]
async fn upload_init_uses_server_default_chunk_size_when_client_omits_it() {
    let app = TestApp::new().await;

    let init_response = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/upload/init",
            json!({
                "path": "docs",
                "filename": "default-chunk.bin",
                "size": 6 * 1024 * 1024,
            }),
        )
        .await;
    assert_eq!(init_response.status(), StatusCode::OK);

    let payload = response_json(init_response).await;
    assert_eq!(payload["chunk_size"], Value::from(5 * 1024 * 1024));
    assert_eq!(payload["total_chunks"], Value::from(2));
}

#[tokio::test]
async fn upload_init_allows_files_up_to_4_gib_and_rejects_larger_sizes() {
    let app = TestApp::new().await;
    let max_upload_size = 4_u64 * 1024 * 1024 * 1024;

    let allowed = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/upload/init",
            json!({
                "path": "docs",
                "filename": "max-size.bin",
                "size": max_upload_size,
            }),
        )
        .await;
    assert_eq!(allowed.status(), StatusCode::OK);

    let rejected = app
        .json_request_as_admin(
            Method::POST,
            "/api/files/upload/init",
            json!({
                "path": "docs",
                "filename": "too-large.bin",
                "size": max_upload_size + 1,
            }),
        )
        .await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

    let payload = response_json(rejected).await;
    assert_eq!(
        payload["code"],
        Value::String("VALIDATION_FAILED".to_string())
    );
    assert!(
        payload["message"]
            .as_str()
            .is_some_and(|message| message.contains(&max_upload_size.to_string())),
        "validation message should mention 4GiB limit"
    );
}

#[tokio::test]
async fn file_routes_require_authenticated_user_when_auth_enabled() {
    let app = TestApp::new().await;

    let tree = app
        .request(
            Request::builder()
                .uri("/api/files/tree")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(tree.status(), StatusCode::UNAUTHORIZED);

    let upload_init = app
        .json_request(
            Method::POST,
            "/api/files/upload/init",
            json!({
                "path": "docs",
                "filename": "anonymous.bin",
                "size": 4,
                "chunk_size": 4,
            }),
        )
        .await;
    assert_eq!(upload_init.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn chunked_upload_rejects_non_owner() {
    let app = TestApp::new().await;
    app.register_user("alice", "alice@example.com", "alice-password")
        .await;
    app.register_user("bob", "bob@example.com", "bob-password")
        .await;

    let alice_cookie = app.login_cookie("alice", "alice-password").await;
    let bob_cookie = app.login_cookie("bob", "bob-password").await;

    let init_response = app
        .json_request_with_cookie(
            Method::POST,
            "/api/files/upload/init",
            json!({
                "path": "docs",
                "filename": "shared.bin",
                "size": 8,
                "chunk_size": 4,
            }),
            &alice_cookie,
        )
        .await;
    assert_eq!(init_response.status(), StatusCode::OK);
    let init_payload = response_json(init_response).await;
    let upload_id = init_payload["upload_id"]
        .as_str()
        .expect("upload_id should be present");

    let chunk_response = app
        .bytes_request_with_cookie(
            Method::PUT,
            &format!("/api/files/upload/chunks/{}/0", upload_id),
            b"part".to_vec(),
            &bob_cookie,
        )
        .await;
    assert_eq!(chunk_response.status(), StatusCode::FORBIDDEN);

    let complete_response = app
        .json_request_with_cookie(
            Method::POST,
            &format!("/api/files/upload/complete/{}", upload_id),
            json!({ "message": "should fail" }),
            &bob_cookie,
        )
        .await;
    assert_eq!(complete_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn backend_serves_static_frontend_and_spa_fallback() {
    let app = TestApp::new_with_static_frontend("<html><body>vfiles-ui</body></html>").await;

    let root_response = app
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(root_response.status(), StatusCode::OK);
    assert_eq!(
        root_response
            .headers()
            .get(header::CACHE_CONTROL)
            .expect("index cache control should be present"),
        "no-cache"
    );
    let root_body = String::from_utf8_lossy(&response_bytes(root_response).await).to_string();
    assert!(root_body.contains("vfiles-ui"));

    let asset_response = app
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/assets/app.js")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(asset_response.status(), StatusCode::OK);
    assert_eq!(
        asset_response
            .headers()
            .get(header::CACHE_CONTROL)
            .expect("asset cache control should be present"),
        "public, max-age=31536000, immutable"
    );
    let asset_body = String::from_utf8_lossy(&response_bytes(asset_response).await).to_string();
    assert!(asset_body.contains("console.log"));

    let spa_response = app
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/login")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(spa_response.status(), StatusCode::OK);
    let spa_body = String::from_utf8_lossy(&response_bytes(spa_response).await).to_string();
    assert!(spa_body.contains("vfiles-ui"));
}

#[tokio::test]
async fn share_disable_requires_share_owner() {
    let app = TestApp::new().await;
    app.upload_version("docs", "share.txt", b"shared file\n", "share source")
        .await;
    app.register_user("member", "member@example.com", "member-password")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let member_cookie = app.login_cookie("member", "member-password").await;

    let share = app
        .json_request_with_cookie(
            Method::POST,
            "/api/share/shares",
            json!({ "path": "docs/share.txt" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(share.status(), StatusCode::OK);
    let share_payload = response_json(share).await;
    let code = share_payload["code"]
        .as_str()
        .expect("share code should be present");

    let forbidden_disable = app
        .request_with_cookie(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/share/shares/{}", code))
                .body(Body::empty())
                .expect("request should build"),
            &member_cookie,
        )
        .await;
    assert_eq!(forbidden_disable.status(), StatusCode::FORBIDDEN);

    let owner_disable = app
        .request_with_cookie(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/share/shares/{}", code))
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(owner_disable.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn admin_route_prevents_deleting_last_admin() {
    let app = TestApp::new().await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    let users_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(users_response.status(), StatusCode::OK);
    let users_payload = response_json(users_response).await;
    let admin_id = users_payload["users"]
        .as_array()
        .expect("users should be an array")
        .iter()
        .find(|user| user["role"] == Value::String("admin".to_string()))
        .and_then(|user| user["id"].as_str())
        .expect("admin id should be present");

    let delete_response = app
        .request_with_cookie(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/admin/users/{}", admin_id))
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(delete_response.status(), StatusCode::CONFLICT);
    let body = String::from_utf8(response_bytes(delete_response).await.to_vec())
        .expect("conflict response should be utf-8");
    assert!(body.contains("Cannot delete the last admin user"));
}

#[tokio::test]
async fn admin_cannot_demote_the_last_admin() {
    let app = TestApp::new().await;
    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    let users_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(users_response.status(), StatusCode::OK);
    let users_payload = response_json(users_response).await;
    let admin_id = users_payload["users"]
        .as_array()
        .expect("users should be an array")
        .iter()
        .find(|user| user["role"] == Value::String("admin".to_string()))
        .and_then(|user| user["id"].as_str())
        .expect("admin id should be present")
        .to_string();

    let response = app
        .json_request_with_cookie(
            Method::PUT,
            &format!("/api/admin/users/{}", admin_id),
            json!({ "role": "user" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = String::from_utf8(response_bytes(response).await.to_vec())
        .expect("conflict response should be utf-8");
    assert!(body.contains("Cannot change the last admin user role"));
}

#[tokio::test]
async fn manager_can_access_admin_routes_but_cannot_promote_users_to_manager() {
    let app = TestApp::new().await;
    app.register_user("operator", "operator@example.com", "operator-password")
        .await;
    app.register_user("member", "member@example.com", "member-password")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;

    let users_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(users_response.status(), StatusCode::OK);
    let users_payload = response_json(users_response).await;
    let operator_id = users_payload["users"]
        .as_array()
        .expect("users should be an array")
        .iter()
        .find(|user| user["username"] == Value::String("operator".to_string()))
        .and_then(|user| user["id"].as_str())
        .expect("operator id should be present")
        .to_string();
    let member_id = users_payload["users"]
        .as_array()
        .expect("users should be an array")
        .iter()
        .find(|user| user["username"] == Value::String("member".to_string()))
        .and_then(|user| user["id"].as_str())
        .expect("member id should be present")
        .to_string();

    let promote_operator = app
        .json_request_with_cookie(
            Method::PUT,
            &format!("/api/admin/users/{}", operator_id),
            json!({ "role": "manager" }),
            &admin_cookie,
        )
        .await;
    assert_eq!(promote_operator.status(), StatusCode::NO_CONTENT);

    let manager_cookie = app.login_cookie("operator", "operator-password").await;
    let manager_list = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
            &manager_cookie,
        )
        .await;
    assert_eq!(manager_list.status(), StatusCode::OK);

    let forbidden = app
        .json_request_with_cookie(
            Method::PUT,
            &format!("/api/admin/users/{}", member_id),
            json!({ "role": "manager" }),
            &manager_cookie,
        )
        .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_can_update_user_email_and_member_can_login_with_new_email() {
    let app = TestApp::new().await;
    app.register_user("member", "member@example.com", "member-password")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let users_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(users_response.status(), StatusCode::OK);
    let users_payload = response_json(users_response).await;
    let member_id = users_payload["users"]
        .as_array()
        .expect("users should be an array")
        .iter()
        .find(|user| user["username"] == Value::String("member".to_string()))
        .and_then(|user| user["id"].as_str())
        .expect("member id should be present")
        .to_string();

    let update_response = app
        .json_request_with_cookie(
            Method::PUT,
            &format!("/api/admin/users/{}", member_id),
            json!({
                "email": "member.updated@example.com",
            }),
            &admin_cookie,
        )
        .await;
    assert_eq!(update_response.status(), StatusCode::NO_CONTENT);

    let user_response = app
        .request_with_cookie(
            Request::builder()
                .uri(format!("/api/admin/users/{}", member_id))
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(user_response.status(), StatusCode::OK);
    let user_payload = response_json(user_response).await;
    assert_eq!(
        user_payload["email"],
        Value::String("member.updated@example.com".to_string())
    );

    let member_cookie = app
        .login_cookie("member.updated@example.com", "member-password")
        .await;
    let me_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/auth/me")
                .body(Body::empty())
                .expect("request should build"),
            &member_cookie,
        )
        .await;
    assert_eq!(me_response.status(), StatusCode::OK);
    let me_payload = response_json(me_response).await;
    assert_eq!(
        me_payload["data"]["user"]["username"],
        Value::String("member".to_string())
    );
    assert_eq!(
        me_payload["data"]["user"]["email"],
        Value::String("member.updated@example.com".to_string())
    );
}

#[tokio::test]
async fn admin_can_revoke_user_sessions() {
    let app = TestApp::new().await;
    app.register_user("member", "member@example.com", "member-password")
        .await;

    let admin_cookie = app.login_cookie("admin", "admin-password").await;
    let member_cookie = app.login_cookie("member", "member-password").await;

    let users_response = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(users_response.status(), StatusCode::OK);
    let users_payload = response_json(users_response).await;
    let member_id = users_payload["users"]
        .as_array()
        .expect("users should be an array")
        .iter()
        .find(|user| user["username"] == Value::String("member".to_string()))
        .and_then(|user| user["id"].as_str())
        .expect("member id should be present")
        .to_string();

    let before_revoke = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/auth/me")
                .body(Body::empty())
                .expect("request should build"),
            &member_cookie,
        )
        .await;
    assert_eq!(before_revoke.status(), StatusCode::OK);
    let before_payload = response_json(before_revoke).await;
    assert_eq!(
        before_payload["data"]["user"]["username"],
        Value::String("member".to_string())
    );

    let revoke_response = app
        .request_with_cookie(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/admin/users/{}/revoke-sessions", member_id))
                .body(Body::empty())
                .expect("request should build"),
            &admin_cookie,
        )
        .await;
    assert_eq!(revoke_response.status(), StatusCode::NO_CONTENT);

    let after_revoke = app
        .request_with_cookie(
            Request::builder()
                .uri("/api/auth/me")
                .body(Body::empty())
                .expect("request should build"),
            &member_cookie,
        )
        .await;
    assert_eq!(after_revoke.status(), StatusCode::OK);
    let after_payload = response_json(after_revoke).await;
    assert_eq!(after_payload["data"]["user"], Value::Null);
}

fn png_fixture(width: u32, height: u32) -> Vec<u8> {
    use image::{ImageFormat, Rgb, RgbImage};

    let mut image = RgbImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
    }

    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("png fixture should encode");
    cursor.into_inner()
}

async fn upload_fixture(app: &TestApp, filename: &str, path: &str, bytes: &[u8]) {
    let (body, content_type) = single_upload_multipart(filename, path, "fixture", bytes);
    let response = app
        .request_as_admin(
            Request::builder()
                .method(Method::POST)
                .uri("/api/files/upload")
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("request should build"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn thumbnail_generates_cached_jpeg_and_supports_conditional_requests() {
    let app = TestApp::new().await;
    let png = png_fixture(400, 300);
    upload_fixture(&app, "photo.png", "docs", &png).await;

    let uri = "/api/files/thumbnail?path=docs/photo.png&size=128";
    let response = app
        .request_as_admin(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("content-type should be present"),
        "image/jpeg"
    );
    let cache_control = response
        .headers()
        .get(header::CACHE_CONTROL)
        .expect("cache-control should be present")
        .to_str()
        .expect("cache-control should be ascii");
    assert!(cache_control.contains("max-age"));
    let etag = response
        .headers()
        .get(header::ETAG)
        .expect("etag should be present")
        .clone();

    let thumbnail = response_bytes(response).await;
    assert_eq!(
        image::guess_format(&thumbnail).expect("thumbnail format should be detected"),
        image::ImageFormat::Jpeg
    );
    let decoded = image::load_from_memory(&thumbnail).expect("thumbnail should decode");
    assert_eq!((decoded.width(), decoded.height()), (128, 96));

    // A second request is served from the on-disk cache and stays byte-identical.
    let cached = app
        .request_as_admin(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(cached.status(), StatusCode::OK);
    assert_eq!(response_bytes(cached).await, thumbnail);

    // Conditional requests revalidate with the ETag instead of resending bytes.
    let not_modified = app
        .request_as_admin(
            Request::builder()
                .uri(uri)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn thumbnail_rejects_non_image_files() {
    let app = TestApp::new().await;
    upload_fixture(&app, "notes.txt", "docs", b"not an image").await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/thumbnail?path=docs/notes.txt")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn thumbnail_requires_authentication() {
    let app = TestApp::new().await;
    let png = png_fixture(64, 64);
    upload_fixture(&app, "photo.png", "docs", &png).await;

    let response = app
        .request(
            Request::builder()
                .uri("/api/files/thumbnail?path=docs/photo.png")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn static_frontend_is_compressed_for_clients_that_accept_it() {
    let app = TestApp::new_with_static_frontend(
        "<html><body>vfiles-ui-compression-check-that-is-long-enough</body></html>",
    )
    .await;

    let response = app
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .header(header::ACCEPT_ENCODING, "gzip, br")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let encoding = response
        .headers()
        .get(header::CONTENT_ENCODING)
        .expect("static asset should be compressed")
        .to_str()
        .expect("content-encoding should be ascii");
    assert!(
        encoding == "gzip" || encoding == "br",
        "unexpected content-encoding: {encoding}"
    );

    // Without Accept-Encoding the body is served uncompressed.
    let identity = app
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(identity.status(), StatusCode::OK);
    assert!(identity.headers().get(header::CONTENT_ENCODING).is_none());
}

#[tokio::test]
async fn range_requests_stay_uncompressed_even_with_accept_encoding() {
    let app = TestApp::new().await;
    app.upload_version("docs", "range-enc.txt", b"0123456789", "range upload")
        .await;

    let response = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/content?path=docs/range-enc.txt")
                .header(header::RANGE, "bytes=2-5")
                .header(header::ACCEPT_ENCODING, "gzip, br")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert!(
        response.headers().get(header::CONTENT_ENCODING).is_none(),
        "partial responses must not be re-encoded"
    );
    assert_eq!(response_bytes(response).await.as_ref(), b"2345");
}

#[tokio::test]
async fn precompressed_static_assets_are_served_when_supported() {
    let app = TestApp::new_with_static_frontend("<html><body>vfiles-ui</body></html>").await;

    let brotli = app
        .request(
            Request::builder()
                .uri("/assets/app.js")
                .header(header::ACCEPT_ENCODING, "gzip, br")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(brotli.status(), StatusCode::OK);
    assert_eq!(
        brotli
            .headers()
            .get(header::CONTENT_ENCODING)
            .expect("content-encoding should be present"),
        "br"
    );
    assert!(
        brotli
            .headers()
            .get(header::VARY)
            .expect("vary should be present")
            .to_str()
            .expect("vary should be ascii")
            .contains("accept-encoding")
    );
    assert_eq!(
        response_bytes(brotli).await.as_ref(),
        b"BROTLI:console.log('vfiles');"
    );

    let gzip = app
        .request(
            Request::builder()
                .uri("/assets/app.js")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(gzip.status(), StatusCode::OK);
    assert_eq!(
        gzip.headers()
            .get(header::CONTENT_ENCODING)
            .expect("content-encoding should be present"),
        "gzip"
    );
    assert_eq!(
        response_bytes(gzip).await.as_ref(),
        b"GZIP:console.log('vfiles');"
    );

    let raw = app
        .request(
            Request::builder()
                .uri("/assets/app.js")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(raw.status(), StatusCode::OK);
    assert!(raw.headers().get(header::CONTENT_ENCODING).is_none());
    assert_eq!(
        response_bytes(raw).await.as_ref(),
        b"console.log('vfiles');"
    );
}

#[tokio::test]
async fn paginated_directory_listing_reports_total_and_pages() {
    let app = TestApp::new().await;
    for index in 0..5 {
        let response = app
            .json_request_as_admin(
                Method::POST,
                "/api/files/directories",
                json!({ "path": format!("p{index}") }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let first = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list?limit=2&offset=0")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(first.status(), StatusCode::OK);
    let payload = response_json(first).await;
    assert_eq!(payload["total"], Value::from(5));
    assert_eq!(payload["limit"], Value::from(2));
    assert_eq!(payload["offset"], Value::from(0));
    assert_eq!(payload["has_more"], Value::Bool(true));
    assert_eq!(payload["items"].as_array().map(Vec::len), Some(2));

    let last = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list?limit=2&offset=4")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let payload = response_json(last).await;
    assert_eq!(payload["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(payload["has_more"], Value::Bool(false));

    // limit 会被钳制到 1000，返回全部条目
    let clamped = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list?limit=9999")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    let payload = response_json(clamped).await;
    assert_eq!(payload["limit"], Value::from(1000));
    assert_eq!(payload["items"].as_array().map(Vec::len), Some(5));

    // 子目录路径形式
    let nested = app
        .request_as_admin(
            Request::builder()
                .uri("/api/files/list/p0?limit=10")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await;
    assert_eq!(nested.status(), StatusCode::OK);
    let payload = response_json(nested).await;
    assert_eq!(payload["total"], Value::from(0));
}
