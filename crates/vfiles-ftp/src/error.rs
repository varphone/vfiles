//! 领域错误到 FTP 应答的映射。
//!
//! 客户端看到的是 FTP 状态码，映射表要保证：
//! - 永久性错误返回 5xx（不要诱导客户端重试）；
//! - 临时性问题（限流、上传会话冲突）返回 4xx，客户端可以重试；
//! - 认证/权限问题返回 550，避免泄露资源是否存在。

use unftp_core::storage::{Error, ErrorKind};
use vfiles_domain::DomainError;

/// 把领域错误转换为 libunftp 的错误类型。
pub fn to_ftp_error(err: DomainError) -> Error {
    let (kind, message) = match &err {
        DomainError::NotFound { resource } => (
            ErrorKind::PermanentFileNotAvailable,
            format!("资源不存在: {resource}"),
        ),
        DomainError::EntryNotFound => (ErrorKind::PermanentFileNotAvailable, err.to_string()),
        DomainError::VersionNotFound => (ErrorKind::PermanentFileNotAvailable, err.to_string()),
        DomainError::SnapshotNotFound => (ErrorKind::PermanentFileNotAvailable, err.to_string()),
        DomainError::PathConflict { .. }
        | DomainError::Conflict { .. }
        | DomainError::PreconditionFailed => {
            (ErrorKind::PermanentFileNotAvailable, err.to_string())
        }
        DomainError::Validation { .. } => (ErrorKind::PermissionDenied, err.to_string()),
        DomainError::Unauthorized
        | DomainError::Forbidden
        | DomainError::Authentication { .. }
        | DomainError::InvalidCredentials
        | DomainError::UserDisabled
        | DomainError::SessionExpired
        | DomainError::SessionRevoked => (ErrorKind::PermissionDenied, err.to_string()),
        DomainError::StorageQuotaExceeded => {
            (ErrorKind::InsufficientStorageSpaceError, err.to_string())
        }
        DomainError::RateLimited => (ErrorKind::TransientFileNotAvailable, err.to_string()),
        DomainError::UploadExpired
        | DomainError::UploadConflict
        | DomainError::UploadPartInvalid
        | DomainError::UploadPartChecksumMismatch
        | DomainError::BlobChecksumMismatch
        | DomainError::SearchIndexNotReady
        | DomainError::NotImplemented { .. }
        | DomainError::Internal { .. } => (ErrorKind::LocalError, err.to_string()),
    };

    Error::new(kind, message)
}

/// 传输中断（客户端断开）应返回可重试错误。
pub fn transfer_aborted(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::ConnectionClosed, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_domain_errors_to_ftp_error_kinds() {
        assert_eq!(
            to_ftp_error(DomainError::NotFound {
                resource: "entry".to_string()
            })
            .kind(),
            ErrorKind::PermanentFileNotAvailable
        );
        assert_eq!(
            to_ftp_error(DomainError::PathConflict {
                message: "occupied".to_string()
            })
            .kind(),
            ErrorKind::PermanentFileNotAvailable
        );
        assert_eq!(
            to_ftp_error(DomainError::StorageQuotaExceeded).kind(),
            ErrorKind::InsufficientStorageSpaceError
        );
        assert_eq!(
            to_ftp_error(DomainError::RateLimited).kind(),
            ErrorKind::TransientFileNotAvailable
        );
        assert_eq!(
            to_ftp_error(DomainError::Forbidden).kind(),
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            to_ftp_error(DomainError::Internal {
                message: "boom".to_string()
            })
            .kind(),
            ErrorKind::LocalError
        );
    }
}
