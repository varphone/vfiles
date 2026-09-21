//! API routes for VFiles.

use axum::{Router, routing::get};
use axum_extra::extract::cookie::CookieJar;

use crate::{
    AppState,
    error::{ApiError, ApiResult},
};
use vfiles_app::NamespaceService;
use vfiles_domain::{AuthUser, DomainError, NamespaceId, UserId};

pub mod admin;
pub mod audit;
pub mod auth;
pub mod content;
pub mod download;
pub mod favorites;
pub mod ftp;
pub mod health;
pub mod history;
pub mod overview;
pub mod search;
pub mod session;
pub mod share;
pub mod snapshot;
pub mod thumbnail;
pub mod tree;
pub mod upload;

#[derive(Debug, Clone)]
pub(crate) struct RequestContext {
    pub namespace_id: NamespaceId,
    pub actor_user_id: UserId,
    /// 已认证用户的用户名（用于审计日志快照）；未启用认证时为 `None`。
    pub username: Option<String>,
}

pub(crate) async fn request_context(
    state: &AppState,
    jar: &CookieJar,
) -> ApiResult<RequestContext> {
    let auth_user = optional_auth_user(state, jar).await?;
    request_context_from_auth_user(state, auth_user).await
}

pub(crate) async fn protected_request_context(
    state: &AppState,
    jar: &CookieJar,
) -> ApiResult<RequestContext> {
    if state.config.auth.enabled {
        authenticated_request_context(state, jar).await
    } else {
        request_context(state, jar).await
    }
}

pub(crate) async fn authenticated_request_context(
    state: &AppState,
    jar: &CookieJar,
) -> ApiResult<RequestContext> {
    let auth_user = require_auth_user(state, jar).await?;
    request_context_from_auth_user(state, Some(auth_user)).await
}

async fn optional_auth_user(state: &AppState, jar: &CookieJar) -> ApiResult<Option<AuthUser>> {
    let Some(auth_service) = &state.auth_service else {
        return Ok(None);
    };

    let Some(cookie) = jar.get("auth_token") else {
        return Ok(None);
    };

    match auth_service.authenticate_session(cookie.value()).await {
        Ok(auth_user) => Ok(Some(auth_user)),
        Err(
            DomainError::Unauthorized
            | DomainError::InvalidCredentials
            | DomainError::UserDisabled
            | DomainError::SessionExpired
            | DomainError::SessionRevoked
            | DomainError::NotFound { .. },
        ) => Ok(None),
        Err(err) => Err(ApiError::Domain(err)),
    }
}

async fn require_auth_user(state: &AppState, jar: &CookieJar) -> ApiResult<AuthUser> {
    let auth_service = state
        .auth_service
        .as_ref()
        .ok_or(ApiError::Domain(DomainError::Unauthorized))?;
    let cookie = jar
        .get("auth_token")
        .ok_or(ApiError::Domain(DomainError::Unauthorized))?;

    match auth_service.authenticate_session(cookie.value()).await {
        Ok(auth_user) => Ok(auth_user),
        Err(
            DomainError::Unauthorized
            | DomainError::InvalidCredentials
            | DomainError::UserDisabled
            | DomainError::SessionExpired
            | DomainError::SessionRevoked
            | DomainError::NotFound { .. },
        ) => Err(ApiError::Domain(DomainError::Unauthorized)),
        Err(err) => Err(ApiError::Domain(err)),
    }
}

async fn request_context_from_auth_user(
    state: &AppState,
    auth_user: Option<AuthUser>,
) -> ApiResult<RequestContext> {
    let namespace_id = if state.config.auth.enabled && state.config.features.multi_user {
        match auth_user.as_ref() {
            Some(auth_user) => ensure_default_namespace(state, &auth_user.id).await?,
            None => state.default_namespace_id,
        }
    } else {
        state.default_namespace_id
    };

    let actor_user_id = auth_user
        .as_ref()
        .map(|auth_user| auth_user.id)
        .unwrap_or(state.default_actor_user_id);
    let username = auth_user
        .as_ref()
        .map(|auth_user| auth_user.username.to_string());

    Ok(RequestContext {
        namespace_id,
        actor_user_id,
        username,
    })
}

/// 取用户默认命名空间（不存在则创建）。
///
/// 实现已下沉到 `vfiles_app::NamespaceService`，FTP 认证后走同一逻辑；
/// 这里只做一次 `Arc<dyn NamespaceRepo>` 的适配与错误转换。
async fn ensure_default_namespace(state: &AppState, owner_id: &UserId) -> ApiResult<NamespaceId> {
    let service = NamespaceService::new(state.namespace_repo.clone());
    service
        .ensure_default_for_owner(owner_id)
        .await
        .map_err(ApiError::Domain)
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/session", session::router())
        .nest("/files", files_router())
        .nest("/history", history::router())
        .nest("/download", download::router())
        .nest("/share", share::router())
        .nest("/admin", admin::router())
        .nest("/audit", audit::router())
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
}

pub fn files_router() -> Router<AppState> {
    Router::new()
        .merge(tree::router())
        .merge(content::router())
        .merge(thumbnail::router())
        .merge(upload::router())
        .merge(snapshot::router())
        .merge(search::router())
        .merge(overview::router())
        .merge(favorites::router())
        .merge(ftp::router())
}
