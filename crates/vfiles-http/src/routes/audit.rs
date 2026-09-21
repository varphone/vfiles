//! 审计日志查询接口。
//!
//! **只读**：这里只提供 GET，没有修改/删除接口；数据库触发器也会拒绝
//! 任何 UPDATE / DELETE，双重保证审计记录不可篡改。

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use vfiles_domain::{AuditLog, AuditLogQuery, AuditResult, DomainError};

use crate::{
    AppState,
    dto::format_timestamp,
    error::{ApiError, ApiResult},
    routes::admin::require_admin,
};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/logs", get(list_logs))
        .route("/actions", get(list_actions))
}

#[derive(Debug, Deserialize)]
pub struct ListAuditQuery {
    keyword: Option<String>,
    action: Option<String>,
    result: Option<String>,
    since: Option<String>,
    until: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct AuditLogDto {
    pub id: String,
    pub created_at: String,
    pub user_id: Option<String>,
    pub username: String,
    pub action: String,
    pub result: String,
    pub target: Option<String>,
    pub ip: Option<String>,
    pub device: Option<String>,
    pub user_agent: Option<String>,
    pub detail: Option<String>,
}

impl From<AuditLog> for AuditLogDto {
    fn from(log: AuditLog) -> Self {
        Self {
            id: log.id,
            created_at: format_timestamp(log.created_at),
            user_id: log.user_id.map(|id| id.to_string()),
            username: log.username,
            action: log.action,
            result: log.result.as_str().to_string(),
            target: log.target,
            ip: log.ip,
            device: log.device,
            user_agent: log.user_agent,
            detail: log.detail,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuditLogListDto {
    pub items: Vec<AuditLogDto>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
}

fn parse_time(value: Option<&str>, field: &str) -> Result<Option<OffsetDateTime>, ApiError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(raw) => OffsetDateTime::parse(raw, &Rfc3339).map(Some).map_err(|_| {
            ApiError::Domain(DomainError::Validation {
                message: format!("Invalid {field} format, expected RFC3339"),
            })
        }),
        None => Ok(None),
    }
}

async fn list_logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListAuditQuery>,
) -> ApiResult<Json<AuditLogListDto>> {
    require_admin(&state, &jar).await?;

    let result = match query.result.as_deref().map(str::trim) {
        Some("success") => Some(AuditResult::Success),
        Some("failure") => Some(AuditResult::Failure),
        Some("") | None => None,
        Some(other) => {
            return Err(ApiError::Domain(DomainError::Validation {
                message: format!("Unknown audit result: {other}"),
            }));
        }
    };

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let filter = AuditLogQuery {
        keyword: query.keyword,
        action: query.action,
        result,
        since: parse_time(query.since.as_deref(), "since")?,
        until: parse_time(query.until.as_deref(), "until")?,
        limit,
        offset: query.offset.unwrap_or(0),
    };

    let page = state.audit_service.list(&filter).await?;

    Ok(Json(AuditLogListDto {
        items: page.items.into_iter().map(AuditLogDto::from).collect(),
        total: page.total,
        limit,
        offset: filter.offset,
    }))
}

async fn list_actions(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<Vec<String>>> {
    require_admin(&state, &jar).await?;
    let actions = state
        .audit_service
        .actions()
        .await
        .map_err(ApiError::Domain)?;
    Ok(Json(actions))
}
