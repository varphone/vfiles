//! 所有权转移接口。
//!
//! `POST /api/files/transfer` 把当前用户名下的文件/目录转给另一个用户；
//! 版本历史随条目一起转移（`entry_versions` 按条目 ID 关联）。
//! `GET /api/users/directory` 提供选择目标用户所需的最小用户列表。

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use vfiles_domain::{DomainError, NewAuditLog, NormalizedPath};

use crate::error::{ApiError, ApiJson, ApiResult};
use crate::{AppState, routes::protected_request_context};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/transfer", post(transfer_ownership))
        .route("/users/directory", get(list_transfer_targets))
}

#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub paths: Vec<String>,
    pub target_user_id: String,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransferResponse {
    pub transferred: usize,
    pub target_username: String,
}

#[derive(Debug, Serialize)]
pub struct TransferTargetDto {
    pub id: String,
    pub username: String,
}

async fn transfer_ownership(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    ApiJson(req): ApiJson<TransferRequest>,
) -> ApiResult<Json<TransferResponse>> {
    let ctx = protected_request_context(&state, &jar).await?;

    let target_user_id = vfiles_domain::UserId::from_string(&req.target_user_id).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid target user id".to_string(),
        })
    })?;

    let mut paths = Vec::with_capacity(req.paths.len());
    for raw in &req.paths {
        let path = NormalizedPath::new(raw).map_err(|_| {
            ApiError::Domain(DomainError::Validation {
                message: format!("Invalid path format: {raw}"),
            })
        })?;
        paths.push(path);
    }

    let actor_username = ctx.username.clone().unwrap_or_default();
    let outcome = state
        .ownership_service
        .transfer(
            &ctx.namespace_id,
            &paths,
            &target_user_id,
            &ctx.actor_user_id,
            &actor_username,
            req.message.as_deref(),
        )
        .await?;

    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::FILE_TRANSFER)
            .user(
                Some(ctx.actor_user_id),
                ctx.username.clone().unwrap_or_default(),
            )
            .target(req.paths.join("、"))
            .detail(format!(
                "转移给 {}（{} 个条目，含版本历史）",
                outcome.target_username, outcome.transferred
            )),
    )
    .await;

    Ok(Json(TransferResponse {
        transferred: outcome.transferred,
        target_username: outcome.target_username,
    }))
}

async fn list_transfer_targets(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(_query): Query<serde_json::Value>,
) -> ApiResult<Json<Vec<TransferTargetDto>>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let targets = state
        .ownership_service
        .list_targets(&ctx.actor_user_id)
        .await?;

    Ok(Json(
        targets
            .into_iter()
            .map(|(id, username)| TransferTargetDto {
                id: id.to_string(),
                username,
            })
            .collect(),
    ))
}
