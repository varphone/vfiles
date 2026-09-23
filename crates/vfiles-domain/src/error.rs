use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Validation failed: {message}")]
    Validation { message: String },

    #[error("Conflict: {message}")]
    Conflict { message: String },

    #[error("Not found: {resource}")]
    NotFound { resource: String },

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Authentication failed: {message}")]
    Authentication { message: String },

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("User disabled")]
    UserDisabled,

    #[error("Session expired")]
    SessionExpired,

    #[error("Session revoked")]
    SessionRevoked,

    #[error("Entry not found")]
    EntryNotFound,

    #[error("Version not found")]
    VersionNotFound,

    #[error("Snapshot not found")]
    SnapshotNotFound,

    #[error("Path conflict: {message}")]
    PathConflict { message: String },

    #[error("Upload expired")]
    UploadExpired,

    #[error("Upload part invalid")]
    UploadPartInvalid,

    #[error("Upload part checksum mismatch")]
    UploadPartChecksumMismatch,

    #[error("Upload conflict")]
    UploadConflict,

    #[error("Storage quota exceeded")]
    StorageQuotaExceeded,

    #[error("Search index not ready")]
    SearchIndexNotReady,

    #[error("Rate limited")]
    RateLimited,

    #[error("Not implemented: {feature}")]
    NotImplemented { feature: String },

    #[error("Internal error: {message}")]
    Internal { message: String },
}

pub type DomainResult<T> = Result<T, DomainError>;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Invalid email: {0}")]
    InvalidEmail(String),

    #[error("Invalid username: {0}")]
    InvalidUsername(String),

    #[error("Empty message")]
    EmptyMessage,
}

impl From<ValidationError> for DomainError {
    fn from(err: ValidationError) -> Self {
        DomainError::Validation {
            message: err.to_string(),
        }
    }
}
