//! Authentication routes.

use axum::{
    Json, Router,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use serde_json::json;

use crate::middleware::login_rate_limit_policy;
use crate::{
    AppState,
    dto::{LoginResponseDto, UserDto},
    error::{ApiError, ApiJson, ApiResult, ErrorResponse},
    middleware::client_ip_from_headers,
};
use vfiles_domain::{DomainError, NewAuditLog, UserRepo};

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
    ApiJson(req): ApiJson<vfiles_domain::LoginRequest>,
) -> ApiResult<Response> {
    let login_identifier = req.username_or_email.clone();
    let login_key = login_rate_limit_key(&headers, &login_identifier);
    let login_rate_limit = login_rate_limit_policy(&state.config.auth.login_rate_limit);

    if let Some(block) = state
        .login_attempt_limiter
        .check(&login_rate_limit, &login_key)
    {
        tracing::warn!(
            "Blocked login attempt for {} due to repeated failures; retry after {}s",
            login_identifier,
            block.retry_after_secs
        );
        crate::audit::record(
            &state,
            &headers,
            NewAuditLog::failure(crate::audit::action::LOGIN_FAILURE)
                .user(None, login_identifier.clone())
                .detail("登录尝试过于频繁，已被限流"),
        )
        .await;
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
                .record_failure(&login_rate_limit, &login_key);
            tracing::warn!("Rejected login attempt for {}", login_identifier);
            crate::audit::record(
                &state,
                &headers,
                NewAuditLog::failure(crate::audit::action::LOGIN_FAILURE)
                    .user(None, login_identifier.clone())
                    .detail("用户名或密码不正确"),
            )
            .await;
            return Err(ApiError::Domain(DomainError::InvalidCredentials));
        }
        Err(err) => return Err(ApiError::Domain(err)),
    };
    // 审计需要 userId，先取出再转换响应 DTO
    let actor_user_id = response.user.id;
    let response_dto: LoginResponseDto = response.into();

    tracing::info!("Login successful for user: {}", response_dto.user.username);

    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::LOGIN_SUCCESS)
            .user(Some(actor_user_id), response_dto.user.username.clone())
            .detail("登录成功"),
    )
    .await;

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
    ApiJson(req): ApiJson<vfiles_domain::RegisterRequest>,
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
    // `enabled` 表示“是否必须登录”，与 `protected_request_context` 判断一致：
    // 关闭认证（VFILES_AUTH_ENABLED=false）时前端不应再要求登录。
    let auth_enabled = state.config.auth.enabled;
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
            auth_enabled,
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
                auth_enabled,
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
                auth_enabled,
                state.config.auth.allow_register,
                None,
            ));
        }
        Err(err) => return Err(ApiError::Domain(err)),
    };

    Ok(auth_me_response(
        auth_enabled,
        state.config.auth.allow_register,
        Some(user.into()),
    ))
}
