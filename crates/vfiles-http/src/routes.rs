//! API routes for VFiles.

use axum::{Router, routing::get};
use axum_extra::extract::cookie::CookieJar;

use crate::{
    AppState,
    error::{ApiError, ApiResult},
};
use vfiles_app::NamespaceService;
use vfiles_domain::{AuthUser, DomainError, NamespaceId, UserId};

pub mod activity;
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
    /// 本次会话的权限范围；访问令牌与主密码会话为全量。
    pub scope: vfiles_domain::SessionScope,
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

async fn optional_auth_user(
    state: &AppState,
    jar: &CookieJar,
) -> ApiResult<Option<vfiles_domain::AuthenticatedSession>> {
    // 访问令牌优先：`Authorization: Bearer vfat_...` 由中间件复制成 vfiles_token cookie
    if let Some(token) = jar.get(crate::middleware::ACCESS_TOKEN_COOKIE)
        && let Some(auth_user) = authenticate_access_token(state, token.value()).await?
    {
        // 访问令牌由主密码会话创建，代表账号本身：全量权限。
        return Ok(Some(vfiles_domain::AuthenticatedSession {
            user: auth_user,
            scope: vfiles_domain::SessionScope::Full,
            credential_id: None,
        }));
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
) -> ApiResult<vfiles_domain::AuthenticatedSession> {
    if jar.get(crate::middleware::ACCESS_TOKEN_COOKIE).is_some() {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }
    require_auth_user(state, jar).await
}

/// 受限会话越权时的统一文案。
pub(crate) const SCOPE_FORBIDDEN_MESSAGE: &str = "该路径不在当前密码的访问范围内";

impl RequestContext {
    /// 读取路径必须在会话范围内。
    pub(crate) fn require_read(&self, path: &str) -> ApiResult<()> {
        if self.scope.allows_path_read(path) {
            Ok(())
        } else {
            Err(ApiError::forbidden(SCOPE_FORBIDDEN_MESSAGE))
        }
    }

    /// 写入路径必须在会话范围内，且受限凭据需要 `can_write`。
    pub(crate) fn require_write(&self, path: &str) -> ApiResult<()> {
        if self.scope.allows_path_write(path) {
            Ok(())
        } else {
            Err(ApiError::forbidden(SCOPE_FORBIDDEN_MESSAGE))
        }
    }

    /// 账号级操作（令牌、管理、概览、转移、手动快照、凭据管理）只允许主密码会话。
    pub(crate) fn require_full(&self) -> ApiResult<()> {
        if self.scope.is_full() {
            Ok(())
        } else {
            Err(ApiError::forbidden(
                "此操作需要主密码会话；受限密码只能访问授权目录",
            ))
        }
    }
}

/// 主密码会话的请求上下文：账号级操作使用。
pub(crate) async fn primary_request_context(
    state: &AppState,
    jar: &CookieJar,
) -> ApiResult<RequestContext> {
    let ctx = protected_request_context(state, jar).await?;
    ctx.require_full()?;
    Ok(ctx)
}

/// 需要主密码会话：受限密码只能浏览/写入授权目录，不能执行账号级操作。
pub(crate) async fn require_primary_session(
    state: &AppState,
    jar: &CookieJar,
) -> ApiResult<AuthUser> {
    let session = require_session_auth_user(state, jar).await?;
    if !session.scope.is_full() {
        return Err(ApiError::forbidden(
            "This action requires the primary account password",
        ));
    }
    Ok(session.user)
}

/// 需要登录：会话 Cookie 或访问令牌任一有效即可。
///
/// 复用 [`optional_auth_user`]，保证两条鉴权路径（会话 / 令牌）行为一致。
async fn require_auth_user(
    state: &AppState,
    jar: &CookieJar,
) -> ApiResult<vfiles_domain::AuthenticatedSession> {
    optional_auth_user(state, jar)
        .await?
        .ok_or(ApiError::Domain(DomainError::Unauthorized))
}

async fn request_context_from_auth_user(
    state: &AppState,
    session: Option<vfiles_domain::AuthenticatedSession>,
) -> ApiResult<RequestContext> {
    let namespace_id = if state.config.auth.enabled && state.config.features.multi_user {
        match session.as_ref() {
            Some(session) => ensure_default_namespace(state, &session.user.id).await?,
            None => state.default_namespace_id,
        }
    } else {
        state.default_namespace_id
    };

    let actor_user_id = session
        .as_ref()
        .map(|session| session.user.id)
        .unwrap_or(state.default_actor_user_id);
    let username = session
        .as_ref()
        .map(|session| session.user.username.to_string());
    let scope = session
        .map(|session| session.scope)
        .unwrap_or(vfiles_domain::SessionScope::Full);

    Ok(RequestContext {
        namespace_id,
        actor_user_id,
        username,
        scope,
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
        // 个人活动流：登录即可访问，不需要管理员权限
        .merge(activity::router())
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
        // 最近修改的文件与概览同源，但条数与范围语义不同，单独注册
        .route("/recent", get(overview::recent))
        .merge(favorites::router())
        .merge(ftp::router())
        .merge(transfer::router())
}
