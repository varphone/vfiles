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
pub mod client_error;
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
pub mod tokens;
pub mod transfer;
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
    // 访问令牌优先：`Authorization: Bearer vfat_...` 由中间件复制成 vfiles_token cookie
    if let Some(token) = jar.get(crate::middleware::ACCESS_TOKEN_COOKIE)
        && let Some(auth_user) = authenticate_access_token(state, token.value()).await?
    {
        return Ok(Some(auth_user));
    }

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

/// 用访问令牌鉴权：校验令牌并把所属用户组装成 [`AuthUser`]。
///
/// 用户被禁用时视为无效令牌。
async fn authenticate_access_token(
    state: &AppState,
    plaintext: &str,
) -> ApiResult<Option<AuthUser>> {
    let Some(token) = state
        .access_token_service
        .authenticate(plaintext)
        .await
        .map_err(ApiError::Domain)?
    else {
        return Ok(None);
    };

    let user = match state.user_repo.find_by_id(&token.user_id).await {
        Ok(user) => user,
        Err(DomainError::NotFound { .. }) => return Ok(None),
        Err(err) => return Err(ApiError::Domain(err)),
    };
    if user.disabled {
        return Ok(None);
    }

    Ok(Some(AuthUser {
        id: user.id,
        username: user.username,
        email: user.email,
        role: user.role,
    }))
}

/// 仅会话（Cookie）鉴权：令牌不能用来创建/撤销令牌，避免泄露后自我扩大。
pub(crate) async fn require_session_auth_user(
    state: &AppState,
    jar: &CookieJar,
) -> ApiResult<AuthUser> {
    if jar.get(crate::middleware::ACCESS_TOKEN_COOKIE).is_some() {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }
    require_auth_user(state, jar).await
}

/// 需要登录：会话 Cookie 或访问令牌任一有效即可。
///
/// 复用 [`optional_auth_user`]，保证两条鉴权路径（会话 / 令牌）行为一致。
async fn require_auth_user(state: &AppState, jar: &CookieJar) -> ApiResult<AuthUser> {
    optional_auth_user(state, jar)
        .await?
        .ok_or(ApiError::Domain(DomainError::Unauthorized))
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
        // 访问令牌是用户级资源，放在 /api/tokens 而不是 /api/files 下
        .merge(tokens::router())
        .merge(crate::routes::client_error::router())
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
        .merge(transfer::router())
}
