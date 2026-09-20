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
            db_pool: pool,
            namespace_repo: Arc::new(namespace_repo),
            entry_repo: Arc::new(entry_repo),
            snapshot_repo: Arc::new(snapshot_repo),
            blob_store: Arc::new(blob_store),
            upload_store: Arc::new(upload_store),
            login_attempt_limiter: Arc::new(LoginAttemptLimiter::new()),
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
    let zip_bytes = response_bytes(folder_download).await;
    let cursor = Cursor::new(zip_bytes.to_vec());
    let mut archive = zip::ZipArchive::new(cursor).expect("zip archive should open");
    assert_eq!(archive.len(), 1);

    let mut file = archive
        .by_name("docs/note.txt")
        .expect("zip should contain uploaded file");
    let mut extracted = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut extracted).expect("zip entry should read");
    assert_eq!(extracted, b"hello from version two\n");

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
    let results = payload
        .as_array()
        .expect("search response should be an array");

    assert_eq!(results.len(), 1);
    assert!(results.iter().any(|item| {
        item["entry"]["path"] == Value::String("packages/2.4.3a0.tgz".to_string())
            && item["entry"]["name"] == Value::String("2.4.3a0.tgz".to_string())
    }));
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
    let results = payload
        .as_array()
        .expect("search response should be an array");
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
