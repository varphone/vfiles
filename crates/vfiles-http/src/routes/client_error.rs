//! 客户端错误上报（r179 ✓ 可靠性面收口）。
//!
//! 前端全局错误边界 fire-and-forget 上报 → tracing::warn 留痕（运维日志面收集 ✓
//! 不落库 ✗ 简式合理；入库/统计 = 后续按需）。认证复用保护上下文（同文件路由式）。

use axum::extract::State;
use axum::{Json, Router, http::StatusCode, routing::post};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::{AppState, error::ApiResult, routes::protected_request_context};

pub fn router() -> Router<AppState> {
    Router::new().route("/client-errors", post(report))
}

#[derive(Debug, Deserialize)]
pub struct ClientErrorReport {
    pub source: String,
    pub message: String,
}

async fn report(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(report): Json<ClientErrorReport>,
) -> ApiResult<StatusCode> {
    let _ctx = protected_request_context(&state, &jar).await?;
    // 截断防滥用（消息上限 500 ✗ 狂写风险）
    let message: String = report.message.chars().take(500).collect();
    tracing::warn!(
        source = %report.source,
        message = %message,
        "客户端错误上报（前端边界收口）"
    );
    Ok(StatusCode::NO_CONTENT)
}
