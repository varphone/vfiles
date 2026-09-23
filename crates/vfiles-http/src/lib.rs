//! HTTP handlers and routing for VFiles.

pub mod audit;
pub mod dto;
pub mod error;
pub mod frontend;
mod http_headers;
pub mod middleware;
pub mod routes;

use axum::{
    Router,
    extract::Request,
    extract::State,
    http::{Method, StatusCode, Uri, header},
    middleware::Next,
    response::IntoResponse,
    response::Response,
};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use vfiles_app::{
    AdminService, AuthService, HealthService, HistoryService, SearchService, SessionService,
    ShareService, UploadService, WorkspaceService,
};
use vfiles_config::AppConfig;
use vfiles_domain::{
    BlobStore, EntryRepo, FavoriteRepo, NamespaceId, NamespaceRepo, SnapshotRepo, UploadStore,
    UserId,
};
use vfiles_infra_sqlite::{
    FsBlobStore, FsUploadStore, SqliteAdminRepo, SqliteEntryRepo, SqlitePool, SqliteSearchRepo,
    SqliteShareRepo, SqliteSnapshotRepo, SqliteUserRepo,
};

pub use frontend::FrontendAssets;
pub use middleware::{
    FixedWindowLimiter, LoginAttemptLimiter, RateLimitPolicy, login_rate_limit_policy,
};

#[derive(Clone)]
pub struct AppState {
    pub health_service: HealthService,
    pub session_service: SessionService,
    pub auth_service: Option<AuthService>,
    pub admin_service: Option<AdminService<SqliteAdminRepo>>,
    pub search_service: SearchService<SqliteSearchRepo<FsBlobStore>>,
    pub share_service: ShareService<SqliteShareRepo, SqliteEntryRepo>,
    pub history_service:
        HistoryService<SqliteEntryRepo, SqliteSnapshotRepo, FsBlobStore, SqliteUserRepo>,
    pub workspace_service:
        WorkspaceService<SqliteEntryRepo, SqliteSnapshotRepo, FsBlobStore, FsUploadStore>,
    pub upload_service:
        UploadService<SqliteEntryRepo, SqliteSnapshotRepo, FsBlobStore, FsUploadStore>,
    pub db_pool: SqlitePool,
    pub namespace_repo: Arc<dyn NamespaceRepo + Send + Sync>,
    pub entry_repo: Arc<dyn EntryRepo + Send + Sync>,
    pub favorite_repo: Arc<dyn FavoriteRepo + Send + Sync>,
    pub snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
    pub blob_store: Arc<dyn BlobStore + Send + Sync>,
    pub upload_store: Arc<dyn UploadStore + Send + Sync>,
    pub login_attempt_limiter: Arc<LoginAttemptLimiter>,
    /// FTP 批量导入的运行计数（由 bin 装配注入，HTTP 与 FTP 共用同一实例）。
    pub ingest_stats: Arc<vfiles_app::IngestStats>,
    pub share_download_limiter: Arc<FixedWindowLimiter>,
    /// 用户仓储（令牌鉴权时补全用户信息）。
    pub user_repo: std::sync::Arc<dyn vfiles_domain::UserRepo + Send + Sync>,
    /// 访问令牌：CLI / 构建系统使用的 API 凭证。
    pub access_token_service: vfiles_app::AccessTokenService,
    /// 所有权转移（文件/目录交给另一个用户，版本历史随行）。
    pub ownership_service: vfiles_app::OwnershipService,
    /// 审计日志：只追加、只查询（数据库触发器禁止修改/删除）。
    pub audit_service: vfiles_app::AuditService<vfiles_infra_sqlite::SqliteAuditLogRepo>,
    pub default_namespace_id: NamespaceId,
    pub default_actor_user_id: UserId,
    pub frontend_assets: Option<FrontendAssets>,
    pub config: AppConfig,
}

pub fn build_router(state: AppState) -> Router<()> {
    build_router_inner(state, true)
}

/// Build the HTTP router without the frontend catch-all so an outer protocol dispatcher can
/// handle unmatched paths first (for example, path-style S3 requests on the same listener).
pub fn build_router_without_frontend(state: AppState) -> Router<()> {
    build_router_inner(state, false)
}

fn build_router_inner(state: AppState, serve_frontend_fallback: bool) -> Router<()> {
    let mut router = Router::new()
        .route(
            "/s/{code}",
            axum::routing::get(routes::share::download_share),
        )
        .nest("/api", routes::api_router());

    if serve_frontend_fallback
        && state
            .frontend_assets
            .as_ref()
            .is_some_and(FrontendAssets::is_available)
    {
        router = router.fallback(serve_frontend);
    }

    router
        .layer(build_compression_layer())
        .layer(build_cors_layer(&state.config))
        .layer(axum::middleware::from_fn(request_logger))
        .layer(axum::middleware::from_fn(
            middleware::security_headers_middleware,
        ))
        .layer(axum::middleware::from_fn(middleware::request_id_middleware))
        // 放在最内层：把 Bearer 令牌归一化成 cookie 后再进处理函数
        .layer(axum::middleware::from_fn(
            middleware::bearer_token_middleware,
        ))
        .with_state(state)
}

/// 压缩层作用于整个路由（含静态前端资源与 `/api`）。
///
/// 静态资源此前完全未压缩：首屏 CSS/JS 会以数百 KB 明文传输，这是最大的首屏瓶颈。
/// 这里在 `DefaultPredicate`（跳过图片/SSE/gRPC 与 <32B 的响应）之外，再排除视频、
/// 音频、PDF、压缩包与 `application/octet-stream`，避免对已压缩内容做无谓的 CPU 开销。
/// Range / `Content-Range` 响应由 tower-http 自动跳过，下载的断点续传不受影响。
fn build_compression_layer() -> CompressionLayer<impl Predicate> {
    let predicate = DefaultPredicate::new()
        .and(NotForContentType::new("audio/"))
        .and(NotForContentType::new("video/"))
        .and(NotForContentType::const_new("application/pdf"))
        .and(NotForContentType::const_new("application/zip"))
        .and(NotForContentType::const_new("application/gzip"))
        .and(NotForContentType::const_new("application/x-7z-compressed"))
        .and(NotForContentType::const_new("application/x-rar-compressed"))
        .and(NotForContentType::const_new("application/octet-stream"));

    CompressionLayer::new().compress_when(predicate)
}

fn build_cors_layer(config: &AppConfig) -> CorsLayer {
    let cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::ACCEPT, header::CONTENT_TYPE]);

    if config.http.cors_allow_any_origin {
        cors.allow_origin(Any)
    } else {
        let origins = config
            .http
            .effective_cors_allowed_origins()
            .into_iter()
            .map(|origin| {
                axum::http::HeaderValue::from_str(&origin)
                    .expect("validated CORS origin should be a valid header value")
            })
            .collect::<Vec<_>>();

        cors.allow_origin(AllowOrigin::list(origins))
            .allow_credentials(true)
    }
}

async fn serve_frontend(
    State(state): State<AppState>,
    uri: Uri,
    headers: axum::http::HeaderMap,
) -> Response {
    let Some(frontend_assets) = state.frontend_assets.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let accept_encoding = headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join(",");

    frontend_assets
        .serve(
            uri.path(),
            (!accept_encoding.is_empty()).then_some(&accept_encoding),
        )
        .await
}

async fn request_logger(req: Request, next: Next) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let request_id = req
        .extensions()
        .get::<middleware::RequestId>()
        .map(|id| id.0.clone())
        .unwrap_or_else(|| "-".to_string());

    tracing::info!(request_id = %request_id, "{} {}", method, uri);

    let response = next.run(req).await;

    tracing::info!(request_id = %request_id, "{} {} -> {}", method, uri, response.status());

    Ok(response)
}
