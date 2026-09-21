//! 访问令牌管理接口（创建/列出/撤销）。
//!
//! 这些接口**只接受会话（Cookie）鉴权**：即使某个令牌泄露，也无法用它创建
//! 新令牌或撤销他人的令牌。明文令牌只在创建响应里返回一次。

use axum::extract::{Path, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use vfiles_app::ALLOWED_EXPIRY_DAYS;
use vfiles_domain::{AccessToken, DomainError, NewAuditLog};

use crate::AppState;
use crate::error::{ApiError, ApiJson, ApiResult};
use crate::routes::{protected_request_context, require_session_auth_user};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tokens", get(list_tokens).post(create_token))
        .route("/tokens/{id}", delete(revoke_token))
        .route("/tokens/expiry-options", get(list_expiry_options))
}

#[derive(Debug, Serialize)]
pub struct AccessTokenDto {
    pub id: String,
    pub name: String,
    /// 展示用前缀（如 `vfat_1a2b3c4d`），不含完整明文。
    pub token_prefix: String,
    pub scopes: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
    /// 是否仍然可用（未撤销、未过期）。
    pub active: bool,
}

fn to_dto(token: &AccessToken, now: time::OffsetDateTime) -> AccessTokenDto {
    AccessTokenDto {
        id: token.id.to_string(),
        name: token.name.clone(),
        token_prefix: token.token_prefix.clone(),
        scopes: token.scopes.clone(),
        // 统一 RFC3339：前端直接 new Date() 即可解析
        created_at: crate::dto::format_timestamp(token.created_at),
        expires_at: token.expires_at.map(crate::dto::format_timestamp),
        last_used_at: token.last_used_at.map(crate::dto::format_timestamp),
        revoked_at: token.revoked_at.map(crate::dto::format_timestamp),
        active: token.is_active(now),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    /// 有效天数：0 表示永久（默认）。
    #[serde(default)]
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    pub token: AccessTokenDto,
    /// 明文令牌，仅此一次返回。
    pub plaintext: String,
}

#[derive(Debug, Serialize)]
pub struct ExpiryOptionDto {
    pub days: u32,
    pub label: String,
}

async fn list_expiry_options() -> Json<Vec<ExpiryOptionDto>> {
    let options = ALLOWED_EXPIRY_DAYS
        .into_iter()
        .map(|days| ExpiryOptionDto {
            days,
            label: match days {
                0 => "永久".to_string(),
                value if value % 365 == 0 => format!("{} 年", value / 365),
                value if value % 30 == 0 => format!("{} 个月", value / 30),
                value => format!("{value} 天"),
            },
        })
        .collect();

    Json(options)
}

async fn list_tokens(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<Vec<AccessTokenDto>>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let now = time::OffsetDateTime::now_utc();
    let tokens = state
        .access_token_service
        .list(&ctx.actor_user_id)
        .await
        .map_err(ApiError::Domain)?;

    Ok(Json(
        tokens.iter().map(|token| to_dto(token, now)).collect(),
    ))
}

async fn create_token(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    ApiJson(req): ApiJson<CreateTokenRequest>,
) -> ApiResult<Json<CreateTokenResponse>> {
    // 令牌不能创建令牌
    let auth_user = require_session_auth_user(&state, &jar).await?;

    let created = state
        .access_token_service
        .create(&auth_user.id, &req.name, req.expires_in_days)
        .await
        .map_err(ApiError::Domain)?;

    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::TOKEN_CREATE)
            .user(Some(auth_user.id), auth_user.username.to_string())
            .target(created.token.name.clone())
            .detail(format!(
                "创建访问令牌 {}（{}）",
                created.token.token_prefix,
                match created.token.expires_at {
                    Some(expires_at) => format!("有效期至 {expires_at}"),
                    None => "永久有效".to_string(),
                }
            )),
    )
    .await;

    Ok(Json(CreateTokenResponse {
        token: to_dto(&created.token, time::OffsetDateTime::now_utc()),
        plaintext: created.plaintext,
    }))
}

async fn revoke_token(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    Path(id): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    let auth_user = require_session_auth_user(&state, &jar).await?;

    let token_id = vfiles_domain::AccessTokenId::from_string(&id).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid token id".to_string(),
        })
    })?;

    state
        .access_token_service
        .revoke(&auth_user.id, &token_id)
        .await
        .map_err(ApiError::Domain)?;

    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::TOKEN_REVOKE)
            .user(Some(auth_user.id), auth_user.username.to_string())
            .target(id.clone())
            .detail("撤销访问令牌".to_string()),
    )
    .await;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
