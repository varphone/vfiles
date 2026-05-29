//! Error handling for HTTP API.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use vfiles_domain::DomainError;

pub type ApiResult<T> = Result<T, ApiError>;

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
    Validation { field: String, message: String },
    Internal(String),
    NotImplemented,
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        ApiError::Domain(err)
    }
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
                None,
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
            ApiError::Domain(DomainError::Validation { message }) => (
                StatusCode::BAD_REQUEST,
                "VALIDATION_FAILED".to_string(),
                format!("Validation failed: {}", message),
                None,
            ),
            ApiError::Domain(DomainError::Conflict { message }) => (
                StatusCode::CONFLICT,
                "CONFLICT".to_string(),
                format!("Conflict: {}", message),
                None,
            ),
            ApiError::Domain(DomainError::Internal { message: _ }) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR".to_string(),
                "Internal server error".to_string(),
                None,
            ),
            ApiError::Validation { field, message } => (
                StatusCode::BAD_REQUEST,
                "VALIDATION_FAILED".to_string(),
                format!("Validation failed for {}: {}", field, message),
                None,
            ),
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
            request_id: None, // TODO: Add request ID from middleware
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
