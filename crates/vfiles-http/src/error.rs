//! Error handling for HTTP API.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use vfiles_domain::DomainError;

use crate::middleware::REQUEST_ID;

pub type ApiResult<T> = Result<T, ApiError>;

/// 与 `axum::Json` 行为一致，但把请求体解析失败也转换成统一的错误信封。
///
/// 直接用 `axum::Json` 时，非法 JSON/缺字段会返回 axum 自己的 422 且响应体不是
/// `{code, message, details}`，客户端无法按错误码本地化。
pub struct ApiJson<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for ApiJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(
        request: axum::extract::Request,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(request, state).await {
            Ok(axum::Json(value)) => Ok(ApiJson(value)),
            Err(rejection) => Err(ApiError::Validation {
                field: "body".to_string(),
                message: rejection.body_text(),
            }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub request_id: Option<String>,
}

#[derive(Debug)]
pub enum ApiError {
    Domain(DomainError),
    Validation {
        field: String,
        message: String,
    },
    /// 上传/写入超过允许的大小上限（带上结构化上限，便于客户端本地化展示）。
    FileTooLarge {
        limit_bytes: u64,
        size_bytes: u64,
    },
    Forbidden {
        message: String,
    },
    Internal(String),
    NotImplemented,
}

impl ApiError {
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden {
            message: message.into(),
        }
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        ApiError::Domain(err)
    }
}

/// 校验失败的结构化信息：`{field?, reason}`。
///
/// 客户端按错误码渲染中文，并可用 `field` 指出具体字段（如 `password`），
/// 无需从英文消息里猜。
/// 只带原因的结构化信息：`{reason}`。
fn reason_details(reason: &str) -> Option<serde_json::Value> {
    validation_details(None, reason)
}

fn validation_details(field: Option<&str>, reason: &str) -> Option<serde_json::Value> {
    let mut details = serde_json::Map::new();
    if let Some(field) = field.filter(|value| !value.is_empty()) {
        details.insert(
            "field".to_string(),
            serde_json::Value::String(field.to_string()),
        );
    }
    if !reason.is_empty() {
        details.insert(
            "reason".to_string(),
            serde_json::Value::String(reason.to_string()),
        );
    }

    (!details.is_empty()).then_some(serde_json::Value::Object(details))
}

/// 从 `PathConflict` 的英文消息里取出冲突路径，作为结构化 `details`。
///
/// 领域错误的消息形如 `Path already exists: docs/a.txt`，路径始终在最后一个 `": "`
/// 之后；取不到时返回 `None`，客户端会退回通用文案。
fn path_conflict_detail(message: &str) -> Option<serde_json::Value> {
    let path = message.rsplit(": ").next()?.trim();
    if path.is_empty() || path == message {
        return None;
    }
    Some(serde_json::json!({ "path": path }))
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self {
            ApiError::Domain(DomainError::NotFound { resource }) => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND".to_string(),
                format!("{} not found", resource),
                None,
            ),
            ApiError::Domain(DomainError::Unauthorized) => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED".to_string(),
                "Authentication required".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::Forbidden) => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN".to_string(),
                "Access denied".to_string(),
                None,
            ),
            ApiError::Forbidden { message } => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN".to_string(),
                message,
                None,
            ),
            ApiError::Domain(DomainError::Authentication { message }) => (
                StatusCode::UNAUTHORIZED,
                "AUTHENTICATION_FAILED".to_string(),
                format!("Authentication failed: {}", message),
                None,
            ),
            ApiError::Domain(DomainError::InvalidCredentials) => (
                StatusCode::UNAUTHORIZED,
                "INVALID_CREDENTIALS".to_string(),
                "Invalid username or password".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::UserDisabled) => (
                StatusCode::UNAUTHORIZED,
                "USER_DISABLED".to_string(),
                "User account is disabled".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::SessionExpired) => (
                StatusCode::UNAUTHORIZED,
                "SESSION_EXPIRED".to_string(),
                "Session has expired".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::SessionRevoked) => (
                StatusCode::UNAUTHORIZED,
                "SESSION_REVOKED".to_string(),
                "Session has been revoked".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::EntryNotFound) => (
                StatusCode::NOT_FOUND,
                "ENTRY_NOT_FOUND".to_string(),
                "File or directory not found".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::VersionNotFound) => (
                StatusCode::NOT_FOUND,
                "VERSION_NOT_FOUND".to_string(),
                "Version not found".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::SnapshotNotFound) => (
                StatusCode::NOT_FOUND,
                "SNAPSHOT_NOT_FOUND".to_string(),
                "Snapshot not found".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::PathConflict { message }) => (
                StatusCode::CONFLICT,
                "PATH_CONFLICT".to_string(),
                format!("Path conflict: {}", message),
                // 冲突路径对用户最有用：抽出来放进 details，客户端可拼成中文提示
                path_conflict_detail(&message),
            ),
            ApiError::Domain(DomainError::UploadExpired) => (
                StatusCode::GONE,
                "UPLOAD_EXPIRED".to_string(),
                "Upload session has expired".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::UploadPartInvalid) => (
                StatusCode::BAD_REQUEST,
                "UPLOAD_PART_INVALID".to_string(),
                "Upload part is invalid".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::UploadPartChecksumMismatch) => (
                StatusCode::BAD_REQUEST,
                "UPLOAD_PART_CHECKSUM_MISMATCH".to_string(),
                "Upload part checksum does not match".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::UploadConflict) => (
                StatusCode::CONFLICT,
                "UPLOAD_CONFLICT".to_string(),
                "Upload conflict".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::StorageQuotaExceeded) => (
                StatusCode::INSUFFICIENT_STORAGE,
                "STORAGE_QUOTA_EXCEEDED".to_string(),
                "Storage quota exceeded".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::SearchIndexNotReady) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "SEARCH_INDEX_NOT_READY".to_string(),
                "Search index is not ready".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::RateLimited) => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED".to_string(),
                "Rate limit exceeded".to_string(),
                None,
            ),
            ApiError::Domain(DomainError::NotImplemented { feature }) => (
                StatusCode::NOT_IMPLEMENTED,
                "NOT_IMPLEMENTED".to_string(),
                format!("Feature not implemented: {}", feature),
                None,
            ),
            ApiError::Domain(DomainError::Validation { message }) => {
                let details = validation_details(None, &message);
                (
                    StatusCode::BAD_REQUEST,
                    "VALIDATION_FAILED".to_string(),
                    format!("Validation failed: {}", message),
                    details,
                )
            }
            ApiError::Domain(DomainError::Conflict { message }) => {
                let details = reason_details(&message);
                (
                    StatusCode::CONFLICT,
                    "CONFLICT".to_string(),
                    format!("Conflict: {}", message),
                    details,
                )
            }
            ApiError::Domain(DomainError::Internal { message: _ }) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR".to_string(),
                "Internal server error".to_string(),
                None,
            ),
            ApiError::FileTooLarge {
                limit_bytes,
                size_bytes,
            } => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "FILE_TOO_LARGE".to_string(),
                format!("File too large. Maximum size is {limit_bytes} bytes"),
                Some(serde_json::json!({
                    "limit_bytes": limit_bytes,
                    "size_bytes": size_bytes,
                })),
            ),
            ApiError::Validation { field, message } => {
                let details = validation_details(Some(&field), &message);
                (
                    StatusCode::BAD_REQUEST,
                    "VALIDATION_FAILED".to_string(),
                    format!("Validation failed for {}: {}", field, message),
                    details,
                )
            }
            ApiError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR".to_string(),
                "Internal server error".to_string(),
                None,
            ),
            ApiError::NotImplemented => (
                StatusCode::NOT_IMPLEMENTED,
                "NOT_IMPLEMENTED".to_string(),
                "Feature not implemented yet".to_string(),
                None,
            ),
        };

        let error_response = ErrorResponse {
            code,
            message,
            details,
            request_id: REQUEST_ID.try_with(|request_id| request_id.clone()).ok(),
        };

        (status, Json(error_response)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::ApiError;
    use axum::{http::StatusCode, response::IntoResponse};
    use http_body_util::BodyExt;
    use vfiles_domain::DomainError;

    #[test]
    fn path_conflict_details_carry_the_offending_path() {
        assert_eq!(
            super::path_conflict_detail("Path already exists: docs/a.txt"),
            Some(serde_json::json!({ "path": "docs/a.txt" }))
        );
        assert_eq!(
            super::path_conflict_detail("Path is occupied by a file: b.txt"),
            Some(serde_json::json!({ "path": "b.txt" }))
        );
        // 没有可提取的路径时不编造 details
        assert_eq!(super::path_conflict_detail("Path conflict"), None);
        assert_eq!(super::path_conflict_detail(""), None);
    }

    #[tokio::test]
    async fn path_conflict_response_exposes_the_path_in_details() {
        let response = ApiError::Domain(DomainError::PathConflict {
            message: "Path already exists: docs/a.txt".to_string(),
        })
        .into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("body should collect")
            .to_bytes();
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("body should be valid json");

        assert_eq!(payload["code"], "PATH_CONFLICT");
        assert_eq!(payload["details"]["path"], "docs/a.txt");
    }

    #[tokio::test]
    async fn internal_errors_do_not_expose_error_details() {
        let response = ApiError::Internal("database schema mismatch".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("body should collect")
            .to_bytes();
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("body should be valid json");

        assert!(payload["details"].is_null());
        assert!(!String::from_utf8_lossy(&body).contains("database schema mismatch"));
    }

    #[tokio::test]
    async fn domain_internal_errors_do_not_expose_error_details() {
        let response = ApiError::Domain(DomainError::Internal {
            message: "sqlite row decode failure".to_string(),
        })
        .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response
            .into_body()
            .collect()
            .await
            .expect("body should collect")
            .to_bytes();
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("body should be valid json");

        assert!(payload["details"].is_null());
        assert!(!String::from_utf8_lossy(&body).contains("sqlite row decode failure"));
    }
}
