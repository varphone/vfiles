//! 个人活动流：当前登录用户自己的审计记录。
//!
//! 与管理台 `/api/audit/logs` 共用 [`AuditLogDto`]，但强制按当前用户过滤，
//! 不要求管理员权限；受限会话只看到可见范围内的路径活动。

use axum::{extract::State, response::Json, routing::get};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use vfiles_domain::{AuditLogQuery, DomainError, SessionScope};

use crate::{
    AppState,
    error::{ApiError, ApiQuery, ApiResult},
    routes::{
        audit::{AuditLogDto, AuditLogListDto, parse_time},
        protected_request_context,
    },
};

const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 200;

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    filter: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    since: Option<String>,
}

/// `filter` 只影响动作范围；`all` 与 `mine` 等价（本项目没有他人活动可见性）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivityFilter {
    Mine,
    Files,
}

impl ActivityFilter {
    fn parse(value: Option<&str>) -> Result<Self, ApiError> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("all") | Some("mine") => Ok(Self::Mine),
            Some("files") => Ok(Self::Files),
            Some(other) => Err(ApiError::Domain(DomainError::Validation {
                message: format!("Unknown activity filter: {other}"),
            })),
        }
    }

    fn action_prefixes(self) -> Vec<String> {
        match self {
            Self::Mine => Vec::new(),
            Self::Files => vec!["file.".to_string(), "directory.".to_string()],
        }
    }
}

pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route("/activity", get(activity))
}

async fn activity(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiQuery(query): ApiQuery<ActivityQuery>,
) -> ApiResult<Json<AuditLogListDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let filter = ActivityFilter::parse(query.filter.as_deref())?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);

    let filter = AuditLogQuery {
        user_id: Some(ctx.actor_user_id),
        action_prefixes: filter.action_prefixes(),
        since: parse_time(query.since.as_deref(), "since")?,
        limit,
        offset,
        ..Default::default()
    };
    let page = state.audit_service.list(&filter).await?;
    let total = page.total;

    // 受限会话的可见范围是若干路径前缀，SQL 里无法简单表达，因此在分页
    // 结果上按 target 过滤。`total` 保持过滤前的值：翻页偏移仍按完整结果
    // 计算，宁可某一页少几条，也不把范围外的路径带出来。
    let items = page
        .items
        .into_iter()
        .filter(|log| activity_in_scope(&ctx.scope, &log.action, log.target.as_deref()))
        .map(AuditLogDto::from)
        .collect();

    Ok(Json(AuditLogListDto {
        items,
        total,
        limit,
        offset,
    }))
}

/// 受限会话是否能看到一条活动。
///
/// 只有明确以条目路径为 `target` 的动作才按范围判断；登录、账号类动作
/// 不隐藏。`share.disable` 的 target 是分享 ID，也不当作路径。
fn activity_in_scope(scope: &SessionScope, action: &str, target: Option<&str>) -> bool {
    if scope.is_full() {
        return true;
    }
    let Some(target) = target else {
        return true;
    };
    let path_action = action.starts_with("file.")
        || action.starts_with("directory.")
        || action.starts_with("share.");
    if !path_action {
        return true;
    }
    // `share.disable` 的 target 是分享 ID 而不是路径，不能按路径范围判断。
    if action.starts_with("share.") && uuid::Uuid::parse_str(target).is_ok() {
        return true;
    }
    scope.allows_path_read(target)
}
