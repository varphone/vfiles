//! 审计日志的请求侧辅助：把 IP / User-Agent 填进日志并写入。
//!
//! 放在 HTTP 层是因为只有这里有请求头信息（真实来源 IP、客户端 UA），
//! 业务服务本身不需要关心这些。

use axum::http::HeaderMap;
use vfiles_app::AuditService;
use vfiles_domain::NewAuditLog;
use vfiles_infra_sqlite::SqliteAuditLogRepo;

use crate::AppState;
use crate::middleware::client_ip_from_headers;

const MAX_AUDIT_USER_AGENT_CHARS: usize = 512;

/// Bound untrusted text before it is copied into logs or audit display fields.
pub(crate) fn sanitize_log_field(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .take(max_chars)
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// 动作标识常量：与前端筛选、文档保持一致。
pub mod action {
    pub const LOGIN_SUCCESS: &str = "login.success";
    pub const LOGIN_FAILURE: &str = "login.failure";
    pub const LOGOUT: &str = "auth.logout";
    pub const FILE_UPLOAD: &str = "file.upload";
    pub const FILE_DOWNLOAD: &str = "file.download";
    pub const FILE_DELETE: &str = "file.delete";
    pub const FILE_MOVE: &str = "file.move";
    pub const FILE_RENAME: &str = "file.rename";
    pub const FILE_TRANSFER: &str = "file.transfer";
    pub const FILE_RESTORE: &str = "file.restore";
    pub const DIR_CREATE: &str = "directory.create";
    pub const SHARE_CREATE: &str = "share.create";
    pub const SHARE_DISABLE: &str = "share.disable";
    pub const SHARE_DOWNLOAD: &str = "share.download";
    pub const USER_CREATE: &str = "user.create";
    pub const USER_UPDATE: &str = "user.update";
    pub const USER_SESSIONS_REVOKE: &str = "user.sessions_revoke";
    pub const AUDIT_EXPORT: &str = "audit.export";
    pub const TOKEN_CREATE: &str = "token.create";
    pub const TOKEN_REVOKE: &str = "token.revoke";
}

/// 从请求头取出 (IP, User-Agent)。
pub fn request_meta(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    let ip = {
        let value = client_ip_from_headers(headers);
        (!value.is_empty() && value != "unknown").then_some(value)
    };
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| sanitize_log_field(value, MAX_AUDIT_USER_AGENT_CHARS));

    (ip, user_agent)
}

/// 写入一条审计日志（自动带上请求元信息）。失败只记警告，不影响主流程。
pub async fn record(state: &AppState, headers: &HeaderMap, entry: NewAuditLog) {
    let (ip, user_agent) = request_meta(headers);
    state
        .audit_service
        .record(entry.request(ip, user_agent))
        .await;
}

/// 写入与某个请求上下文关联的审计日志（自动带上用户快照）。
pub(crate) async fn record_for(
    state: &AppState,
    headers: &HeaderMap,
    ctx: &crate::routes::RequestContext,
    entry: NewAuditLog,
) {
    let entry = entry.user(
        ctx.username.clone().map(|_| ctx.actor_user_id),
        ctx.username.clone().unwrap_or_default(),
    );
    record(state, headers, entry).await;
}

/// 便于测试与装配处引用具体服务类型。
pub type Audit = AuditService<SqliteAuditLogRepo>;

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::{MAX_AUDIT_USER_AGENT_CHARS, request_meta, sanitize_log_field};

    #[test]
    fn sanitizes_line_breaks_and_other_control_characters() {
        assert_eq!(
            sanitize_log_field("ui\r\nchunk\tload\0failed\u{2028}entry\u{202e}", 100),
            "ui  chunk load failed entry "
        );
    }

    #[test]
    fn bounds_logged_fields_by_unicode_character_count() {
        let sanitized = sanitize_log_field(&"界".repeat(600), 500);

        assert_eq!(sanitized.chars().count(), 500);
    }

    #[test]
    fn bounds_user_agent_in_audit_metadata() {
        let user_agent = format!("browser/{}", "x".repeat(600));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_str(&user_agent).expect("user agent should be a valid header"),
        );

        let (_, recorded_user_agent) = request_meta(&headers);

        assert_eq!(
            recorded_user_agent
                .expect("user agent should be recorded")
                .chars()
                .count(),
            MAX_AUDIT_USER_AGENT_CHARS
        );
    }
}
