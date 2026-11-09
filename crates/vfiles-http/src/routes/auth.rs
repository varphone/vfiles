//! Authentication routes.

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use serde_json::json;

use crate::middleware::login_rate_limit_policy;
use crate::{
    AppState,
    audit::sanitize_log_field,
    dto::{LoginCredentialDto, LoginResponseDto, ScopeDto, UserDto},
    error::{ApiError, ApiJson, ApiPath, ApiResult, ErrorResponse},
    middleware::client_ip_from_headers,
    routes::primary_request_context,
};
use vfiles_domain::{CredentialId, DomainError, EntryKind, NewAuditLog, NormalizedPath, UserRepo};

const MAX_LOGIN_BODY_BYTES: usize = 16 * 1024;
const MAX_LOGIN_IDENTIFIER_LOG_CHARS: usize = 254;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/login",
            post(login).layer(DefaultBodyLimit::max(MAX_LOGIN_BODY_BYTES)),
        )
        .route("/register", post(register))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route(
            "/credentials",
            get(list_login_credentials).post(create_login_credential),
        )
        .route(
            "/credentials/{id}",
            post(update_login_credential).delete(delete_login_credential),
        )
}

fn auth_me_response(
    enabled: bool,
    allow_register: bool,
    user: Option<UserDto>,
    scope: Option<ScopeDto>,
) -> Json<serde_json::Value> {
    Json(json!({
        "success": true,
        "data": {
            "enabled": enabled,
            "allowRegister": allow_register,
            "user": user,
            "scope": scope,
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
    let logged_login_identifier =
        sanitize_log_field(&login_identifier, MAX_LOGIN_IDENTIFIER_LOG_CHARS);
    let login_source_ip = client_ip_from_headers(&headers);
    let login_rate_limit = login_rate_limit_policy(&state.config.auth.login_rate_limit);

    if let Some(block) = state.login_attempt_limiter.check_login(
        &login_rate_limit,
        &login_source_ip,
        &login_identifier,
    ) {
        tracing::warn!(
            "Blocked login attempt for {} due to repeated failures; retry after {}s",
            logged_login_identifier,
            block.retry_after_secs
        );
        crate::audit::record(
            &state,
            &headers,
            NewAuditLog::failure(crate::audit::action::LOGIN_FAILURE)
                .user(None, logged_login_identifier.clone())
                .detail("登录尝试过于频繁，已被限流"),
        )
        .await;
        return Ok(login_rate_limited_response(block.retry_after_secs));
    }

    tracing::info!("Login attempt for user: {}", logged_login_identifier);

    let auth_service =
        state
            .auth_service
            .as_ref()
            .ok_or(ApiError::Domain(DomainError::Authentication {
                message: "Authentication not enabled".to_string(),
            }))?;

    let response: vfiles_domain::LoginResponse = match auth_service.login(req).await {
        Ok(response) => {
            state
                .login_attempt_limiter
                .clear_login_success(&login_source_ip, &login_identifier);
            response
        }
        Err(DomainError::InvalidCredentials) => {
            state.login_attempt_limiter.record_login_failure(
                &login_rate_limit,
                &login_source_ip,
                &login_identifier,
            );
            tracing::warn!("Rejected login attempt for {}", logged_login_identifier);
            crate::audit::record(
                &state,
                &headers,
                NewAuditLog::failure(crate::audit::action::LOGIN_FAILURE)
                    .user(None, logged_login_identifier.clone())
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

    let login_detail = match response_dto.scope.label.as_deref() {
        Some(label) => format!("登录成功（受限密码：{label}）"),
        None => "登录成功".to_string(),
    };
    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::LOGIN_SUCCESS)
            .user(Some(actor_user_id), response_dto.user.username.clone())
            .detail(login_detail),
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
            None,
        ));
    };

    // Get token from cookie
    let Some(token) = jar.get("auth_token") else {
        return Ok(auth_me_response(
            auth_enabled,
            state.config.auth.allow_register,
            None,
            None,
        ));
    };

    let session = match auth_service.authenticate_session(token.value()).await {
        Ok(session) => session,
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
                None,
            ));
        }
        Err(err) => return Err(ApiError::Domain(err)),
    };

    let user: vfiles_domain::User =
        match auth_service.user_repo().find_by_id(&session.user.id).await {
            Ok(user) => user,
            Err(DomainError::NotFound { .. }) => {
                return Ok(auth_me_response(
                    auth_enabled,
                    state.config.auth.allow_register,
                    None,
                    None,
                ));
            }
            Err(err) => return Err(ApiError::Domain(err)),
        };

    Ok(auth_me_response(
        auth_enabled,
        state.config.auth.allow_register,
        Some(user.into()),
        Some(ScopeDto::from(&session.scope)),
    ))
}

// ---- 受限登录密码（同一账号的多条密码） ----

const MAX_SCOPE_ROOTS: usize = 8;
const MIN_CREDENTIAL_PASSWORD_CHARS: usize = 6;
const MAX_CREDENTIAL_PASSWORD_CHARS: usize = 256;
const MAX_CREDENTIAL_LABEL_CHARS: usize = 64;

#[derive(Debug, serde::Deserialize)]
struct CreateLoginCredentialRequest {
    label: String,
    password: String,
    #[serde(default)]
    roots: Vec<String>,
    #[serde(default, alias = "canWrite")]
    can_write: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateLoginCredentialRequest {
    label: Option<String>,
    password: Option<String>,
    roots: Option<Vec<String>>,
    #[serde(alias = "canWrite")]
    can_write: Option<bool>,
}

fn validation_error(message: &str) -> ApiError {
    ApiError::Domain(DomainError::Validation {
        message: message.to_string(),
    })
}

fn auth_service(state: &AppState) -> ApiResult<&vfiles_app::AuthService> {
    state
        .auth_service
        .as_ref()
        .ok_or(ApiError::Domain(DomainError::Authentication {
            message: "Authentication not enabled".to_string(),
        }))
}

fn normalize_label(label: &str) -> ApiResult<String> {
    let trimmed = label.trim();
    let chars = trimmed.chars().count();
    if chars == 0 || chars > MAX_CREDENTIAL_LABEL_CHARS {
        return Err(validation_error("备注长度需在 1-64 个字符之间"));
    }
    Ok(trimmed.to_string())
}

fn validate_credential_password(password: &str) -> ApiResult<()> {
    let chars = password.chars().count();
    if chars < MIN_CREDENTIAL_PASSWORD_CHARS || chars > MAX_CREDENTIAL_PASSWORD_CHARS {
        return Err(validation_error("受限密码长度需在 6-256 个字符之间"));
    }
    Ok(())
}

/// 校验并规范化范围根：必须是当前命名空间内已存在的目录，不能是命名空间根。
async fn normalize_scope_roots(
    state: &AppState,
    namespace_id: &vfiles_domain::NamespaceId,
    raw: &[String],
) -> ApiResult<Vec<NormalizedPath>> {
    if raw.is_empty() {
        return Err(validation_error("至少选择一个允许访问的目录"));
    }
    if raw.len() > MAX_SCOPE_ROOTS {
        return Err(validation_error("最多选择 8 个允许访问的目录"));
    }
    let mut roots: Vec<NormalizedPath> = Vec::new();
    for value in raw {
        let trimmed = value.trim().trim_matches('/');
        if trimmed.is_empty() {
            return Err(validation_error("不能把命名空间根目录作为受限范围"));
        }
        let path =
            NormalizedPath::new(trimmed).map_err(|_| validation_error("目录路径格式不正确"))?;
        let entry = state
            .entry_repo
            .find_by_path(namespace_id, &path)
            .await
            .map_err(ApiError::Domain)?
            .ok_or_else(|| validation_error("目录不存在"))?;
        if entry.entry_type != EntryKind::Directory {
            return Err(validation_error("受限范围必须是目录"));
        }
        if !roots.iter().any(|existing| existing == &path) {
            roots.push(path);
        }
    }
    Ok(roots)
}

async fn list_login_credentials(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<Vec<LoginCredentialDto>>> {
    let ctx = primary_request_context(&state, &jar).await?;
    let credentials = auth_service(&state)?
        .list_login_credentials(&ctx.actor_user_id)
        .await
        .map_err(ApiError::Domain)?;
    Ok(Json(credentials.into_iter().map(Into::into).collect()))
}

async fn create_login_credential(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ApiJson(req): ApiJson<CreateLoginCredentialRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let ctx = primary_request_context(&state, &jar).await?;
    let label = normalize_label(&req.label)?;
    validate_credential_password(&req.password)?;
    let roots = normalize_scope_roots(&state, &ctx.namespace_id, &req.roots).await?;
    let credential_id = auth_service(&state)?
        .create_login_credential(
            &ctx.actor_user_id,
            &label,
            &req.password,
            &roots,
            req.can_write,
        )
        .await
        .map_err(ApiError::Domain)?;

    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::CREDENTIAL_CREATE)
            .user(
                Some(ctx.actor_user_id),
                ctx.username.clone().unwrap_or_default(),
            )
            .target(label.clone())
            .detail(format!(
                "创建受限登录密码「{label}」（{}，{}）",
                roots
                    .iter()
                    .map(|root| root.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                if req.can_write { "可写" } else { "只读" }
            )),
    )
    .await;

    Ok(Json(serde_json::json!({ "id": credential_id.to_string() })))
}

async fn update_login_credential(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ApiPath(id): ApiPath<String>,
    ApiJson(req): ApiJson<UpdateLoginCredentialRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let ctx = primary_request_context(&state, &jar).await?;
    let credential_id =
        CredentialId::from_string(&id).map_err(|_| validation_error("凭据 ID 格式不正确"))?;
    let auth = auth_service(&state)?;
    let existing = auth
        .find_login_credential(&ctx.actor_user_id, &credential_id)
        .await
        .map_err(ApiError::Domain)?;

    let label = match req.label.as_deref() {
        Some(label) => normalize_label(label)?,
        None => existing.label.clone(),
    };
    if let Some(password) = req.password.as_deref() {
        validate_credential_password(password)?;
    }
    let roots = match req.roots.as_deref() {
        Some(raw) => normalize_scope_roots(&state, &ctx.namespace_id, raw).await?,
        None => existing.scope_roots.clone(),
    };
    let can_write = req.can_write.unwrap_or(existing.can_write);

    auth.update_login_credential(
        &ctx.actor_user_id,
        &credential_id,
        &label,
        req.password.as_deref(),
        &roots,
        can_write,
    )
    .await
    .map_err(ApiError::Domain)?;

    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::CREDENTIAL_UPDATE)
            .user(
                Some(ctx.actor_user_id),
                ctx.username.clone().unwrap_or_default(),
            )
            .target(label.clone())
            .detail(format!(
                "更新受限登录密码「{label}」（{}，{}）{}",
                roots
                    .iter()
                    .map(|root| root.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                if can_write { "可写" } else { "只读" },
                if req.password.is_some() {
                    "；密码已轮换，旧会话已失效"
                } else {
                    "；旧会话已失效"
                }
            )),
    )
    .await;

    Ok(Json(serde_json::json!({ "updated": true })))
}

async fn delete_login_credential(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ApiPath(id): ApiPath<String>,
) -> ApiResult<axum::http::StatusCode> {
    let ctx = primary_request_context(&state, &jar).await?;
    let credential_id =
        CredentialId::from_string(&id).map_err(|_| validation_error("凭据 ID 格式不正确"))?;
    let deleted = auth_service(&state)?
        .delete_login_credential(&ctx.actor_user_id, &credential_id)
        .await
        .map_err(ApiError::Domain)?;
    if !deleted {
        return Err(ApiError::Domain(DomainError::NotFound {
            resource: "credential".to_string(),
        }));
    }

    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::CREDENTIAL_DELETE)
            .user(
                Some(ctx.actor_user_id),
                ctx.username.clone().unwrap_or_default(),
            )
            .target(id)
            .detail("删除受限登录密码；其会话已失效"),
    )
    .await;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
