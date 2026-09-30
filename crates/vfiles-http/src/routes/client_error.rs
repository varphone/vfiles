//! 客户端错误上报（r179 ✓ 可靠性面收口）。
//!
//! 前端全局错误边界 fire-and-forget 上报 → tracing::warn 留痕（运维日志面收集 ✓
//! 不落库 ✗ 简式合理；入库/统计 = 后续按需）。认证复用保护上下文（同文件路由式）。

use axum::extract::{DefaultBodyLimit, State};
use axum::{Router, http::StatusCode, routing::post};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::{
    AppState,
    audit::sanitize_log_field,
    error::{ApiJson, ApiResult},
    routes::protected_request_context,
};

const MAX_CLIENT_ERROR_BODY_BYTES: usize = 16 * 1024;
const MAX_CLIENT_ERROR_SOURCE_CHARS: usize = 200;
const MAX_CLIENT_ERROR_MESSAGE_CHARS: usize = 500;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/client-errors",
        post(report).layer(DefaultBodyLimit::max(MAX_CLIENT_ERROR_BODY_BYTES)),
    )
}

#[derive(Debug, Deserialize)]
pub struct ClientErrorReport {
    pub source: String,
    pub message: String,
}

async fn report(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiJson(report): ApiJson<ClientErrorReport>,
) -> ApiResult<StatusCode> {
    let _ctx = protected_request_context(&state, &jar).await?;
    let source = sanitize_log_field(&report.source, MAX_CLIENT_ERROR_SOURCE_CHARS);
    let message = sanitize_log_field(&report.message, MAX_CLIENT_ERROR_MESSAGE_CHARS);
    tracing::warn!(
        source = %source,
        message = %message,
        "客户端错误上报（前端边界收口）"
    );
    Ok(StatusCode::NO_CONTENT)
}
