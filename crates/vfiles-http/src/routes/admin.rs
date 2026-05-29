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

use crate::{AppState, dto::AdminUserSummaryDto};

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

fn parse_role(value: &str) -> Result<Role, (StatusCode, String)> {
    match value {
        "admin" => Ok(Role::Admin),
        "manager" => Ok(Role::Manager),
        "user" => Ok(Role::User),
        _ => Err((StatusCode::BAD_REQUEST, "Invalid role".to_string())),
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

async fn require_admin(
    state: &AppState,
    jar: &CookieJar,
) -> Result<AuthUser, (StatusCode, String)> {
    if state.admin_service.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            "Admin access not available".to_string(),
        ));
    }

    let auth_service = state.auth_service.as_ref().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "Authentication required".to_string(),
        )
    })?;

    let token = jar.get("auth_token").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "Authentication required".to_string(),
        )
    })?;

    let auth_user = auth_service
        .authenticate_session(token.value())
        .await
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                "Authentication required".to_string(),
            )
        })?;

    if !auth_user.role.can_access_admin_panel() {
        return Err((
            StatusCode::FORBIDDEN,
            "Admin or manager role required".to_string(),
        ));
    }

    Ok(auth_user)
}

async fn list_users(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<AdminUserListResponse>, (StatusCode, String)> {
    let _actor = require_admin(&state, &jar).await?;
    let admin_service = state.admin_service.as_ref().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Admin access not available".to_string(),
        )
    })?;

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    if page < 1 || page_size < 1 || page_size > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid pagination parameters".to_string(),
        ));
    }

    match admin_service.list_users(page, page_size).await {
        Ok(user_list) => Ok(Json(AdminUserListResponse {
            users: user_list.users.into_iter().map(Into::into).collect(),
            total_count: user_list.total_count,
            page: user_list.page,
            page_size: user_list.page_size,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list users: {}", e),
        )),
    }
}

async fn create_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, (StatusCode, String)> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state.admin_service.as_ref().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Admin access not available".to_string(),
        )
    })?;

    let role = parse_role(&req.role)?;
    if !can_manage_user(actor.role, Role::User, Some(role)) {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admin can assign admin or manager roles".to_string(),
        ));
    }

    let create_req = vfiles_app::CreateUserRequest {
        username: req.username,
        email: req.email,
        password: req.password,
        role,
    };

    match admin_service.create_user(create_req).await {
        Ok(user_id) => Ok(Json(CreateUserResponse {
            user_id: user_id.to_string(),
        })),
        Err(DomainError::Conflict { message }) => Err((StatusCode::CONFLICT, message)),
        Err(DomainError::Validation { message }) => Err((StatusCode::BAD_REQUEST, message)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create user: {}", e),
        )),
    }
}

async fn get_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
) -> Result<Json<AdminUserSummaryDto>, (StatusCode, String)> {
    let _actor = require_admin(&state, &jar).await?;
    let admin_service = state.admin_service.as_ref().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Admin access not available".to_string(),
        )
    })?;

    let user_id = match UserId::from_string(&user_id) {
        Ok(id) => id,
        Err(_) => return Err((StatusCode::BAD_REQUEST, "Invalid user ID".to_string())),
    };

    match admin_service.get_user_details(&user_id).await {
        Ok(user) => Ok(Json(user.into())),
        Err(DomainError::NotFound { .. }) => {
            Err((StatusCode::NOT_FOUND, "User not found".to_string()))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get user: {}", e),
        )),
    }
}

async fn update_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state.admin_service.as_ref().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Admin access not available".to_string(),
        )
    })?;

    let user_id = match UserId::from_string(&user_id) {
        Ok(id) => id,
        Err(_) => return Err((StatusCode::BAD_REQUEST, "Invalid user ID".to_string())),
    };

    let current_user = admin_service
        .get_user_details(&user_id)
        .await
        .map_err(|err| match err {
            DomainError::NotFound { .. } => (StatusCode::NOT_FOUND, "User not found".to_string()),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get user: {}", err),
            ),
        })?;

    let role = if let Some(role_str) = req.role {
        Some(parse_role(&role_str)?)
    } else {
        None
    };

    if !can_manage_user(actor.role, current_user.role, role) {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admin can manage admin or manager accounts".to_string(),
        ));
    }

    let update_req = vfiles_app::UpdateUserRequest {
        role,
        disabled: req.disabled,
        email: req.email,
    };

    match admin_service.update_user(&user_id, update_req).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(DomainError::NotFound { .. }) => {
            Err((StatusCode::NOT_FOUND, "User not found".to_string()))
        }
        Err(DomainError::Conflict { message }) => Err((StatusCode::CONFLICT, message)),
        Err(DomainError::Validation { message }) => Err((StatusCode::BAD_REQUEST, message)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update user: {}", e),
        )),
    }
}

async fn revoke_user_sessions(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state.admin_service.as_ref().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Admin access not available".to_string(),
        )
    })?;

    let user_id = match UserId::from_string(&user_id) {
        Ok(id) => id,
        Err(_) => return Err((StatusCode::BAD_REQUEST, "Invalid user ID".to_string())),
    };

    let current_user = admin_service
        .get_user_details(&user_id)
        .await
        .map_err(|err| match err {
            DomainError::NotFound { .. } => (StatusCode::NOT_FOUND, "User not found".to_string()),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get user: {}", err),
            ),
        })?;

    if !can_manage_user(actor.role, current_user.role, None) {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admin can manage admin or manager accounts".to_string(),
        ));
    }

    match admin_service.revoke_user_sessions(&user_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(DomainError::NotFound { .. }) => {
            Err((StatusCode::NOT_FOUND, "User not found".to_string()))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to revoke user sessions: {}", e),
        )),
    }
}

async fn delete_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state.admin_service.as_ref().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Admin access not available".to_string(),
        )
    })?;

    let user_id = match UserId::from_string(&user_id) {
        Ok(id) => id,
        Err(_) => return Err((StatusCode::BAD_REQUEST, "Invalid user ID".to_string())),
    };

    let current_user = admin_service
        .get_user_details(&user_id)
        .await
        .map_err(|err| match err {
            DomainError::NotFound { .. } => (StatusCode::NOT_FOUND, "User not found".to_string()),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get user: {}", err),
            ),
        })?;

    if !can_manage_user(actor.role, current_user.role, None) {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admin can manage admin or manager accounts".to_string(),
        ));
    }

    match admin_service.delete_user(&user_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(DomainError::NotFound { .. }) => {
            Err((StatusCode::NOT_FOUND, "User not found".to_string()))
        }
        Err(DomainError::Conflict { message }) => Err((StatusCode::CONFLICT, message)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete user: {}", e),
        )),
    }
}

async fn reset_password(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    Json(req): Json<ResetPasswordRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let actor = require_admin(&state, &jar).await?;
    let admin_service = state.admin_service.as_ref().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "Admin access not available".to_string(),
        )
    })?;

    let user_id = match UserId::from_string(&user_id) {
        Ok(id) => id,
        Err(_) => return Err((StatusCode::BAD_REQUEST, "Invalid user ID".to_string())),
    };

    let current_user = admin_service
        .get_user_details(&user_id)
        .await
        .map_err(|err| match err {
            DomainError::NotFound { .. } => (StatusCode::NOT_FOUND, "User not found".to_string()),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get user: {}", err),
            ),
        })?;

    if !can_manage_user(actor.role, current_user.role, None) {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admin can manage admin or manager accounts".to_string(),
        ));
    }

    match admin_service
        .reset_user_password(&user_id, &req.new_password)
        .await
    {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(DomainError::NotFound { .. }) => {
            Err((StatusCode::NOT_FOUND, "User not found".to_string()))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to reset password: {}", e),
        )),
    }
}
