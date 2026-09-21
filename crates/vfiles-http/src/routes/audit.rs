//! 审计日志查询接口。
//!
//! **只读**：这里只提供 GET，没有修改/删除接口；数据库触发器也会拒绝
//! 任何 UPDATE / DELETE，双重保证审计记录不可篡改。

use axum::body::Body;
use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use vfiles_domain::{AuditLog, AuditLogQuery, AuditResult, DomainError, NewAuditLog};

use crate::{
    AppState,
    dto::format_timestamp,
    error::{ApiError, ApiResult},
    routes::admin::require_admin,
};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

/// 导出上限：一次最多导出的行数（内部按 500 分页拉取）。
const EXPORT_MAX_ROWS: u32 = 10_000;
const EXPORT_PAGE_SIZE: u32 = 500;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/logs", get(list_logs))
        .route("/actions", get(list_actions))
        .route("/logs.csv", get(export_logs_csv))
        .route("/summary", get(summarize_logs))
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
pub struct AuditCountDto {
    pub key: String,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct AuditLogSummaryDto {
    pub total: u64,
    pub failures: u64,
    pub users: Vec<AuditCountDto>,
    pub actions: Vec<AuditCountDto>,
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

/// 把查询参数解析为过滤条件（列表与导出共用）。
fn build_filter(
    query: &ListAuditQuery,
    limit: u32,
    offset: u32,
) -> Result<AuditLogQuery, ApiError> {
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

    Ok(AuditLogQuery {
        keyword: query.keyword.clone(),
        action: query.action.clone(),
        result,
        since: parse_time(query.since.as_deref(), "since")?,
        until: parse_time(query.until.as_deref(), "until")?,
        limit,
        offset,
    })
}

/// 过滤条件的可读描述（写入导出审计记录）。
fn describe_filter(query: &ListAuditQuery) -> String {
    let mut parts = Vec::new();
    if let Some(keyword) = query
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        parts.push(format!("关键字={keyword}"));
    }
    if let Some(action) = query
        .action
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        parts.push(format!("动作={action}"));
    }
    if let Some(result) = query
        .result
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        parts.push(format!("结果={result}"));
    }
    if let Some(since) = query
        .since
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        parts.push(format!("since={since}"));
    }
    if let Some(until) = query
        .until
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        parts.push(format!("until={until}"));
    }

    if parts.is_empty() {
        "全部记录".to_string()
    } else {
        parts.join("，")
    }
}

async fn list_logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListAuditQuery>,
) -> ApiResult<Json<AuditLogListDto>> {
    require_admin(&state, &jar).await?;

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let filter = build_filter(&query, limit, query.offset.unwrap_or(0))?;

    let page = state.audit_service.list(&filter).await?;

    Ok(Json(AuditLogListDto {
        items: page.items.into_iter().map(AuditLogDto::from).collect(),
        total: page.total,
        limit,
        offset: filter.offset,
    }))
}

/// CSV 转义：含分隔符/引号/换行的字段用双引号包裹并转义内部引号。
fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn csv_row(fields: &[String]) -> String {
    fields
        .iter()
        .map(|field| csv_field(field))
        .collect::<Vec<_>>()
        .join(",")
}

/// 导出为 CSV（只读导出；导出动作本身也会写入审计日志）。
async fn export_logs_csv(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    Query(query): Query<ListAuditQuery>,
) -> ApiResult<Response> {
    let actor = require_admin(&state, &jar).await?;

    // 先确认筛选合法（把参数错误返回给调用方）
    build_filter(&query, 1, 0)?;

    let mut rows: Vec<AuditLog> = Vec::new();
    let mut offset = 0;
    while (rows.len() as u32) < EXPORT_MAX_ROWS {
        let page_size = EXPORT_PAGE_SIZE.min(EXPORT_MAX_ROWS - rows.len() as u32);
        let page = state
            .audit_service
            .list(&build_filter(&query, page_size, offset)?)
            .await?;
        let received = page.items.len();
        rows.extend(page.items);
        if received == 0 || rows.len() as u64 >= page.total {
            break;
        }
        offset += received as u32;
    }

    let mut csv = String::from("\u{feff}"); // BOM：便于 Excel 正确识别 UTF-8
    csv.push_str(&csv_row(&[
        "时间".to_string(),
        "用户".to_string(),
        "动作".to_string(),
        "对象".to_string(),
        "结果".to_string(),
        "IP".to_string(),
        "设备".to_string(),
        "说明".to_string(),
    ]));
    csv.push('\n');

    for entry in &rows {
        csv.push_str(&csv_row(&[
            format_timestamp(entry.created_at),
            entry.username.clone(),
            entry.action.clone(),
            entry.target.clone().unwrap_or_default(),
            entry.result.as_str().to_string(),
            entry.ip.clone().unwrap_or_default(),
            entry.device.clone().unwrap_or_default(),
            entry.detail.clone().unwrap_or_default(),
        ]));
        csv.push('\n');
    }

    crate::audit::record(
        &state,
        &headers,
        NewAuditLog::success(crate::audit::action::AUDIT_EXPORT)
            .user(Some(actor.id), actor.username.to_string())
            .detail(format!(
                "导出审计日志 {} 条（{}）",
                rows.len(),
                describe_filter(&query)
            )),
    )
    .await;

    let filename = format!(
        "audit-logs-{}.csv",
        time::OffsetDateTime::now_utc()
            .date()
            .to_string()
            .replace('-', "")
    );

    let mut response = Response::new(Body::from(csv));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| {
                axum::http::HeaderValue::from_static("attachment; filename=\"audit-logs.csv\"")
            }),
    );

    Ok(response)
}

/// Top 用户/动作的数量：概览只展示前几条，避免面板过宽。
const SUMMARY_TOP: u32 = 5;

async fn summarize_logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListAuditQuery>,
) -> ApiResult<Json<AuditLogSummaryDto>> {
    require_admin(&state, &jar).await?;

    let filter = build_filter(&query, 1, 0)?;
    let summary = state
        .audit_service
        .summarize(&filter, SUMMARY_TOP)
        .await
        .map_err(ApiError::Domain)?;

    Ok(Json(AuditLogSummaryDto {
        total: summary.total,
        failures: summary.failures,
        users: summary
            .users
            .into_iter()
            .map(|item| AuditCountDto {
                key: item.key,
                count: item.count,
            })
            .collect(),
        actions: summary
            .actions
            .into_iter()
            .map(|item| AuditCountDto {
                key: item.key,
                count: item.count,
            })
            .collect(),
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
