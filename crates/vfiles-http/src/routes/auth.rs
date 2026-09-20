//! Authentication routes.

use axum::{
    Json, Router,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use serde_json::json;

use crate::{
    AppState,
    dto::{LoginResponseDto, UserDto},
    error::{ApiError, ApiResult, ErrorResponse},
    middleware::client_ip_from_headers,
};
use vfiles_domain::{DomainError, UserRepo};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

fn auth_me_response(
    enabled: bool,
    allow_register: bool,
    user: Option<UserDto>,
) -> Json<serde_json::Value> {
    Json(json!({
        "success": true,
        "data": {
            "enabled": enabled,
            "allowRegister": allow_register,
            "user": user,
        },
    }))
}

pub async fn login(
    jar: CookieJar,
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    Json(req): Json<vfiles_domain::LoginRequest>,
) -> ApiResult<Response> {
    let login_identifier = req.username_or_email.clone();
    let login_key = login_rate_limit_key(&headers, &login_identifier);

    if let Some(block) = state
        .login_attempt_limiter
        .check(&state.config.auth.login_rate_limit, &login_key)
    {
        tracing::warn!(
            "Blocked login attempt for {} due to repeated failures; retry after {}s",
            login_identifier,
            block.retry_after_secs
        );
        return Ok(login_rate_limited_response(block.retry_after_secs));
    }

    tracing::info!("Login attempt for user: {}", login_identifier);

    let auth_service =
        state
            .auth_service
            .as_ref()
            .ok_or(ApiError::Domain(DomainError::Authentication {
                message: "Authentication not enabled".to_string(),
            }))?;

    let response: vfiles_domain::LoginResponse = match auth_service.login(req).await {
        Ok(response) => {
            state.login_attempt_limiter.clear(&login_key);
            response
        }
        Err(DomainError::InvalidCredentials) => {
            state
                .login_attempt_limiter
                .record_failure(&state.config.auth.login_rate_limit, &login_key);
            tracing::warn!("Rejected login attempt for {}", login_identifier);
            return Err(ApiError::Domain(DomainError::InvalidCredentials));
        }
        Err(err) => return Err(ApiError::Domain(err)),
    };
    let response_dto: LoginResponseDto = response.into();

    tracing::info!("Login successful for user: {}", response_dto.user.username);

    // Set session cookie
    let cookie =
        axum_extra::extract::cookie::Cookie::build(("auth_token", response_dto.token.clone()))
            .path("/")
            .http_only(true)
            .secure(state.config.http.cookie_secure())
            .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let jar = jar.add(cookie);

    Ok((jar, Json(response_dto)).into_response())
}

fn login_rate_limit_key(headers: &HeaderMap, username_or_email: &str) -> String {
    let ip = client_ip_from_headers(headers);
    let normalized_login = username_or_email.trim().to_ascii_lowercase();

    if normalized_login.is_empty() {
        format!("ip:{}|login", ip)
    } else {
        format!("ip:{}|login:{}", ip, normalized_login)
    }
}

fn login_rate_limited_response(retry_after_secs: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrorResponse {
            code: "LOGIN_RATE_LIMITED".to_string(),
            message: "Too many login attempts. Please try again later.".to_string(),
            details: None,
            request_id: None,
        }),
    )
        .into_response();

    response.headers_mut().insert(
        header::RETRY_AFTER,
        HeaderValue::from_str(&retry_after_secs.to_string())
            .expect("retry-after header should be valid"),
    );

    response
}

pub async fn register(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<vfiles_domain::RegisterRequest>,
) -> ApiResult<Json<UserDto>> {
    tracing::info!("Registration attempt for user: {}", req.username);

    if !state.config.auth.allow_register {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }

    let auth_service =
        state
            .auth_service
            .as_ref()
            .ok_or(ApiError::Domain(DomainError::Authentication {
                message: "Authentication not enabled".to_string(),
            }))?;

    let user: vfiles_domain::User = auth_service.register(req).await?;
    let user_dto: UserDto = user.into();

    tracing::info!("Registration successful for user: {}", user_dto.username);

    Ok(Json(user_dto))
}

pub async fn logout(
    jar: CookieJar,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> ApiResult<impl IntoResponse> {
    let auth_service =
        state
            .auth_service
            .as_ref()
            .ok_or(ApiError::Domain(DomainError::Authentication {
                message: "Authentication not enabled".to_string(),
            }))?;

    // Get token from cookie
    if let Some(cookie) = jar.get("auth_token") {
        let _ = auth_service.logout(cookie.value()).await; // Ignore errors for logout
    }

    // Clear session cookie
    let cookie = axum_extra::extract::cookie::Cookie::build(("auth_token", ""))
        .path("/")
        .http_only(true)
        .secure(state.config.http.cookie_secure())
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::seconds(0));

    let jar = jar.add(cookie);

    Ok((
        jar,
        Json(serde_json::json!({ "message": "Logged out successfully" })),
    ))
}

pub async fn me(
    jar: CookieJar,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let Some(auth_service) = state.auth_service.as_ref() else {
        return Ok(auth_me_response(
            false,
            state.config.auth.allow_register,
            None,
        ));
    };

    // Get token from cookie
    let Some(token) = jar.get("auth_token") else {
        return Ok(auth_me_response(
            true,
            state.config.auth.allow_register,
            None,
        ));
    };

    let auth_user = match auth_service.authenticate_session(token.value()).await {
        Ok(user) => user,
        Err(
            DomainError::Unauthorized
            | DomainError::InvalidCredentials
            | DomainError::UserDisabled
            | DomainError::SessionExpired
            | DomainError::SessionRevoked
            | DomainError::NotFound { .. },
        ) => {
            return Ok(auth_me_response(
                true,
                state.config.auth.allow_register,
                None,
            ));
        }
        Err(err) => return Err(ApiError::Domain(err)),
    };

    let user: vfiles_domain::User = match auth_service.user_repo().find_by_id(&auth_user.id).await {
        Ok(user) => user,
        Err(DomainError::NotFound { .. }) => {
            return Ok(auth_me_response(
                true,
                state.config.auth.allow_register,
                None,
            ));
        }
        Err(err) => return Err(ApiError::Domain(err)),
    };

    Ok(auth_me_response(
        true,
        state.config.auth.allow_register,
        Some(user.into()),
    ))
}
