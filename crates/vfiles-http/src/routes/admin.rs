use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use vfiles_domain::*;

use crate::{
    AppState,
    dto::AdminUserSummaryDto,
    error::{ApiError, ApiJson, ApiResult},
};

#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AdminUserListResponse {
    pub users: Vec<AdminUserSummaryDto>,
    pub total_count: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub role: String, // "admin", "manager" or "user"
}

#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    /// 改名（用于修复历史数据里的非法用户名）
    pub username: Option<String>,
    pub role: Option<String>, // "admin", "manager" or "user"
    pub disabled: Option<bool>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

/// 系统信息（r106 看板 ✓ 管理员专属 ✓ 零依赖段：版本/运行时/uptime）。
#[derive(Debug, Serialize)]
pub struct SystemInfoResponse {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub uptime_secs: u64,
    pub started_at: String,
    pub webdav_enabled: bool,
    pub webdav_bind: String,
    /// r-new 共端口模式（true = 挂主端口 mount ✗ false = 独立端口 bind）。
    pub webdav_embedded: bool,
    /// 嵌入挂载路径（standalone 忽略 ✗ 默认 /dav）。
    pub webdav_mount: String,
}

fn process_start() -> std::time::Instant {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    *START.get_or_init(std::time::Instant::now)
}

async fn system_info(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<SystemInfoResponse>> {
    let _actor = require_admin(&state, &jar).await?;
    let uptime = process_start().elapsed().as_secs();
    // WebDAV 接入信息（管理员看板 ✓ 与 config 层一致（r109b））。
    let (webdav_enabled, webdav_bind, webdav_embedded, webdav_mount) =
        match vfiles_config::ConfigLoader::load().map(|c| c.webdav) {
            Ok(w) => (w.enabled, w.bind_address(), w.embedded, w.mount_path),
            Err(_) => (false, String::new(), false, "/dav".to_string()),
        };
    Ok(Json(SystemInfoResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        uptime_secs: uptime,
        started_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        webdav_enabled,
        webdav_bind,
        webdav_embedded,
        webdav_mount,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/system-info", get(system_info))
        .route("/users", get(list_users))
        .route("/users", post(create_user))
        .route("/users/{user_id}", get(get_user))
        .route("/users/{user_id}", put(update_user))
        .route("/users/{user_id}", delete(delete_user))
        .route(
            "/users/{user_id}/revoke-sessions",
            post(revoke_user_sessions),
        )
        .route("/users/{user_id}/reset-password", post(reset_password))
}

fn parse_role(value: &str) -> ApiResult<Role> {
    match value {
        "admin" => Ok(Role::Admin),
        "manager" => Ok(Role::Manager),
        "user" => Ok(Role::User),
        _ => Err(ApiError::Validation {
            field: "role".to_string(),
            message: "Invalid role".to_string(),
        }),
    }
}

fn can_manage_user(actor_role: Role, target_role: Role, requested_role: Option<Role>) -> bool {
    if target_role.is_admin() || requested_role.is_some_and(Role::is_admin) {
        return actor_role.is_admin();
    }

    if target_role.is_manager() || requested_role.is_some_and(Role::is_manager) {
        return actor_role.is_admin();
    }

    actor_role.can_access_admin_panel()
}

pub(crate) async fn require_admin(state: &AppState, jar: &CookieJar) -> ApiResult<AuthUser> {
    if state.admin_service.is_none() {
        return Err(ApiError::forbidden("Admin access not available"));
    }

    let auth_service = state
        .auth_service
        .as_ref()
        .ok_or(ApiError::Domain(DomainError::Unauthorized))?;

    let token = jar
        .get("auth_token")
        .ok_or(ApiError::Domain(DomainError::Unauthorized))?;

    let auth_user = auth_service
        .authenticate_session(token.value())
        .await
        .map_err(|_| ApiError::Domain(DomainError::Unauthorized))?;

    if !auth_user.role.can_access_admin_panel() {
        return Err(ApiError::forbidden("Admin or manager role required"));
    }

    Ok(auth_user)
}

/// 管理动作的审计记录：actor 为执行操作的管理员。
async fn record_admin_action(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    actor: &AuthUser,
    action: &str,
    target: String,
    detail: String,
) {
    crate::audit::record(
        state,
        headers,
        NewAuditLog::success(action)
            .user(Some(actor.id), actor.username.to_string())
            .target(target)
            .detail(detail),
    )
    .await;
}

async fn list_users(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListUsersQuery>,
) -> ApiResult<Json<AdminUserListResponse>> {
    let _actor = require_admin(&state, &jar).await?;
    let admin_service = state
        .admin_service
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("Admin access not available"))?;

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    if page < 1 || !(1..=100).contains(&page_size) {
        return Err(ApiError::Validation {
            field: "page/page_size".to_string(),
            message: "Invalid pagination parameters".to_string(),
        });
    }

    let user_list = admin_service.list_users(page, page_size).await?;
    Ok(Json(AdminUserListResponse {
        users: user_list.users.into_iter().map(Into::into).collect(),
        total_count: user_list.total_count,
        page: user_list.page,
        page_size: user_list.page_size,
    }))
}

async fn create_user(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    jar: CookieJar,
    ApiJson(req): ApiJson<CreateUserRequest>,
) -> ApiResult<Json<CreateUserResponse>> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state
        .admin_service
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("Admin access not available"))?;

    let role = parse_role(&req.role)?;
    if !can_manage_user(actor.role, Role::User, Some(role)) {
        return Err(ApiError::forbidden(
            "Only admin can assign admin or manager roles",
        ));
    }

    let create_req = vfiles_app::CreateUserRequest {
        username: req.username,
        email: req.email,
        password: req.password,
        role,
    };

    let created_username = create_req.username.clone();
    let user_id = admin_service.create_user(create_req).await?;

    record_admin_action(
        &state,
        &headers,
        &actor,
        crate::audit::action::USER_CREATE,
        created_username.clone(),
        format!("创建用户 {created_username}（角色 {}）", role),
    )
    .await;

    Ok(Json(CreateUserResponse {
        user_id: user_id.to_string(),
    }))
}

async fn get_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
) -> ApiResult<Json<AdminUserSummaryDto>> {
    let _actor = require_admin(&state, &jar).await?;
    let admin_service = state
        .admin_service
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("Admin access not available"))?;

    let user_id = UserId::from_string(&user_id).map_err(|_| ApiError::Validation {
        field: "user_id".to_string(),
        message: "Invalid user ID".to_string(),
    })?;

    let user = admin_service.get_user_details(&user_id).await?;
    Ok(Json(user.into()))
}

async fn update_user(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    ApiJson(req): ApiJson<UpdateUserRequest>,
) -> ApiResult<StatusCode> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state
        .admin_service
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("Admin access not available"))?;

    let user_id = UserId::from_string(&user_id).map_err(|_| ApiError::Validation {
        field: "user_id".to_string(),
        message: "Invalid user ID".to_string(),
    })?;

    let current_user = admin_service.get_user_details(&user_id).await?;

    let role = if let Some(role_str) = req.role {
        Some(parse_role(&role_str)?)
    } else {
        None
    };

    if !can_manage_user(actor.role, current_user.role, role) {
        return Err(ApiError::forbidden(
            "Only admin can manage admin or manager accounts",
        ));
    }

    let update_req = vfiles_app::UpdateUserRequest {
        username: req.username,
        role,
        disabled: req.disabled,
        email: req.email,
    };

    let target_username = current_user.username.to_string();
    let change = format!(
        "role={:?} disabled={:?} email={:?}",
        update_req.role, update_req.disabled, update_req.email
    );
    admin_service.update_user(&user_id, update_req).await?;

    record_admin_action(
        &state,
        &headers,
        &actor,
        crate::audit::action::USER_UPDATE,
        target_username.clone(),
        format!("更新用户 {target_username}：{change}"),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_user_sessions(
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
) -> ApiResult<StatusCode> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state
        .admin_service
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("Admin access not available"))?;

    let user_id = UserId::from_string(&user_id).map_err(|_| ApiError::Validation {
        field: "user_id".to_string(),
        message: "Invalid user ID".to_string(),
    })?;

    let current_user = admin_service.get_user_details(&user_id).await?;

    if !can_manage_user(actor.role, current_user.role, None) {
        return Err(ApiError::forbidden(
            "Only admin can manage admin or manager accounts",
        ));
    }

    let target_username = current_user.username.to_string();
    admin_service.revoke_user_sessions(&user_id).await?;

    record_admin_action(
        &state,
        &headers,
        &actor,
        crate::audit::action::USER_SESSIONS_REVOKE,
        target_username.clone(),
        format!("强制下线用户 {target_username}"),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
) -> ApiResult<StatusCode> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state
        .admin_service
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("Admin access not available"))?;

    let user_id = UserId::from_string(&user_id).map_err(|_| ApiError::Validation {
        field: "user_id".to_string(),
        message: "Invalid user ID".to_string(),
    })?;

    let current_user = admin_service.get_user_details(&user_id).await?;

    if !can_manage_user(actor.role, current_user.role, None) {
        return Err(ApiError::forbidden(
            "Only admin can manage admin or manager accounts",
        ));
    }

    admin_service.delete_user(&user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_password(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    ApiJson(req): ApiJson<ResetPasswordRequest>,
) -> ApiResult<StatusCode> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state
        .admin_service
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("Admin access not available"))?;

    let user_id = UserId::from_string(&user_id).map_err(|_| ApiError::Validation {
        field: "user_id".to_string(),
        message: "Invalid user ID".to_string(),
    })?;

    let current_user = admin_service.get_user_details(&user_id).await?;

    if !can_manage_user(actor.role, current_user.role, None) {
        return Err(ApiError::forbidden(
            "Only admin can manage admin or manager accounts",
        ));
    }

    admin_service
        .reset_user_password(&user_id, &req.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
