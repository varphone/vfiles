//! HTTP handlers and routing for VFiles.

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
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use vfiles_app::{
    AdminService, AuthService, HealthService, HistoryService, SearchService, SessionService,
    ShareService, UploadService, WorkspaceService,
};
use vfiles_config::AppConfig;
use vfiles_domain::{
    BlobStore, EntryRepo, NamespaceId, NamespaceRepo, SnapshotRepo, UploadStore, UserId,
};
use vfiles_infra_sqlite::{
    FsBlobStore, FsUploadStore, SqliteAdminRepo, SqliteEntryRepo, SqlitePool, SqliteSearchRepo,
    SqliteShareRepo, SqliteSnapshotRepo, SqliteUserRepo,
};

pub use frontend::FrontendAssets;
pub use middleware::LoginAttemptLimiter;

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
    pub snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
    pub blob_store: Arc<dyn BlobStore + Send + Sync>,
    pub upload_store: Arc<dyn UploadStore + Send + Sync>,
    pub login_attempt_limiter: Arc<LoginAttemptLimiter>,
    pub default_namespace_id: NamespaceId,
    pub default_actor_user_id: UserId,
    pub frontend_assets: Option<FrontendAssets>,
    pub config: AppConfig,
}

pub fn build_router(state: AppState) -> Router<()> {
    let api_router = routes::api_router().layer(CompressionLayer::new());

    let mut router = Router::new()
        .route(
            "/s/{code}",
            axum::routing::get(routes::share::download_share),
        )
        .nest("/api", api_router);

    if state
        .frontend_assets
        .as_ref()
        .is_some_and(FrontendAssets::is_available)
    {
        router = router.fallback(serve_frontend);
    }

    router
        .layer(build_cors_layer(&state.config))
        .layer(axum::middleware::from_fn(request_logger))
        .layer(axum::middleware::from_fn(
            middleware::security_headers_middleware,
        ))
        .layer(axum::middleware::from_fn(middleware::request_id_middleware))
        .with_state(state)
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

async fn serve_frontend(State(state): State<AppState>, uri: Uri) -> Response {
    let Some(frontend_assets) = state.frontend_assets.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    frontend_assets.serve(uri.path()).await
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
