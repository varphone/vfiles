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
    error::{ApiError, ApiResult},
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
    pub role: Option<String>, // "admin", "manager" or "user"
    pub disabled: Option<bool>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
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

async fn require_admin(state: &AppState, jar: &CookieJar) -> ApiResult<AuthUser> {
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

    if page < 1 || page_size < 1 || page_size > 100 {
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
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateUserRequest>,
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

    let user_id = admin_service.create_user(create_req).await?;
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
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
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
        role,
        disabled: req.disabled,
        email: req.email,
    };

    admin_service.update_user(&user_id, update_req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_user_sessions(
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

    admin_service.revoke_user_sessions(&user_id).await?;
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
    Json(req): Json<ResetPasswordRequest>,
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
