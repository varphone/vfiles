//! Data transfer objects for HTTP API.

use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use vfiles_app::{AdminUserList, AdminUserSummary, SessionBootstrap, TreeItem};
use vfiles_domain::{
    ChangeType, Entry, EntryKind, EntryVersion, FeatureMatrix, LoginResponse, Snapshot, User,
};

fn format_timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}

fn format_timestamp_opt(value: Option<OffsetDateTime>) -> Option<String> {
    value.map(format_timestamp)
}

#[derive(Debug, Serialize)]
pub struct AdminUserSummaryDto {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub role: String,
    pub disabled: bool,
    pub created_at: String,
    pub last_login: Option<String>,
}

impl From<AdminUserSummary> for AdminUserSummaryDto {
    fn from(user: AdminUserSummary) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username,
            email: user.email,
            role: user.role.to_string(),
            disabled: user.disabled,
            created_at: format_timestamp(user.created_at),
            last_login: format_timestamp_opt(user.last_login),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdminUserListDto {
    pub users: Vec<AdminUserSummaryDto>,
    pub total_count: i64,
    pub page: i64,
    pub page_size: i64,
}

impl From<AdminUserList> for AdminUserListDto {
    fn from(list: AdminUserList) -> Self {
        Self {
            users: list.users.into_iter().map(Into::into).collect(),
            total_count: list.total_count,
            page: list.page,
            page_size: list.page_size,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SessionBootstrapDto {
    pub auth_enabled: bool,
    pub current_user: Option<String>, // Just ID for now
    pub capabilities: Vec<String>,
    pub active_workspace: Option<String>, // Just ID for now
    pub features: FeatureMatrixDto,
}

impl From<SessionBootstrap> for SessionBootstrapDto {
    fn from(bootstrap: SessionBootstrap) -> Self {
        Self {
            auth_enabled: bootstrap.auth_enabled,
            current_user: bootstrap.current_user.map(|id| id.to_string()),
            capabilities: bootstrap
                .capabilities
                .into_iter()
                .map(|c| c.to_string())
                .collect(),
            active_workspace: bootstrap.active_workspace.map(|id| id.to_string()),
            features: bootstrap.features.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FeatureMatrixDto {
    pub auth_enabled: bool,
    pub multi_user: bool,
    pub email_login: bool,
    pub search_content: bool,
    pub share_enabled: bool,
    pub history_enabled: bool,
}

impl From<FeatureMatrix> for FeatureMatrixDto {
    fn from(matrix: FeatureMatrix) -> Self {
        Self {
            auth_enabled: matrix.auth_enabled,
            multi_user: matrix.multi_user,
            email_login: matrix.email_login,
            search_content: matrix.search_content,
            share_enabled: matrix.share_enabled,
            history_enabled: matrix.history_enabled,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UserDto {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub role: String,
    pub disabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<User> for UserDto {
    fn from(user: User) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username.to_string(),
            email: user.email.map(|email| email.to_string()),
            role: user.role.to_string(),
            disabled: user.disabled,
            created_at: format_timestamp(user.created_at),
            updated_at: format_timestamp(user.updated_at),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LoginResponseDto {
    pub user: UserDto,
    pub token: String,
}

impl From<LoginResponse> for LoginResponseDto {
    fn from(response: LoginResponse) -> Self {
        Self {
            user: response.user.into(),
            token: response.token,
        }
    }
}

// File system DTOs
#[derive(Debug, Serialize)]
pub struct EntryDto {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
    pub is_text: Option<bool>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// 分页的目录列表响应（`GET /api/files/list`）。
#[derive(Debug, Serialize)]
pub struct EntryPageDto {
    pub items: Vec<EntryDto>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

impl From<Entry> for EntryDto {
    fn from(entry: Entry) -> Self {
        Self {
            id: entry.id.to_string(),
            path: entry.path_norm.as_str().to_string(),
            name: entry.name.clone(),
            kind: match entry.entry_type {
                EntryKind::File => "file".to_string(),
                EntryKind::Directory => "directory".to_string(),
            },
            size_bytes: None, // Will be filled from version if available
            mime_type: None,  // Will be filled from version if available
            is_text: None,    // Will be filled from version if available
            created_at: format_timestamp(entry.created_at),
            updated_at: None,
        }
    }
}

impl From<TreeItem> for EntryDto {
    fn from(item: TreeItem) -> Self {
        Self {
            id: item.entry_id.to_string(),
            path: item.path,
            name: item.name,
            kind: match item.kind {
                EntryKind::File => "file".to_string(),
                EntryKind::Directory => "directory".to_string(),
            },
            size_bytes: item.size_bytes,
            mime_type: item.mime_type,
            is_text: item.is_text,
            created_at: format_timestamp(item.created_at),
            updated_at: format_timestamp_opt(item.modified_at),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EntryVersionDto {
    pub id: String,
    pub version_no: u32,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
    pub is_text: bool,
    pub content_hash: String,
    pub created_by: String,
    pub created_at: String,
    pub change_type: String,
    pub change_message: Option<String>,
}

impl From<EntryVersion> for EntryVersionDto {
    fn from(version: EntryVersion) -> Self {
        Self {
            id: version.id.to_string(),
            version_no: version.version_no,
            size_bytes: version.size_bytes.as_u64(),
            mime_type: version.mime_type,
            is_text: version.is_text,
            content_hash: version.content_hash.as_str().to_string(),
            created_by: version.created_by.to_string(),
            created_at: format_timestamp(version.created_at),
            change_type: match version.change_type {
                ChangeType::Added => "added".to_string(),
                ChangeType::Modified => "modified".to_string(),
                ChangeType::Deleted => "deleted".to_string(),
                ChangeType::Renamed => "renamed".to_string(),
            },
            change_message: version.change_message.map(|msg| msg.as_str().to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SnapshotDto {
    pub id: String,
    pub snapshot_no: u32,
    pub created_by: String,
    pub created_at: String,
    pub message: Option<String>,
    pub kind: String,
}

impl From<Snapshot> for SnapshotDto {
    fn from(snapshot: Snapshot) -> Self {
        Self {
            id: snapshot.id.to_string(),
            snapshot_no: snapshot.snapshot_no,
            created_by: snapshot.created_by.to_string(),
            created_at: format_timestamp(snapshot.created_at),
            message: snapshot.message.map(|msg| msg.as_str().to_string()),
            kind: match snapshot.kind {
                vfiles_domain::SnapshotKind::UserCreated => "user".to_string(),
                vfiles_domain::SnapshotKind::AutoCommit => "auto".to_string(),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateDirectoryRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct MoveEntryRequest {
    pub from: String,
    pub to: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSnapshotRequest {
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UploadRequest {
    pub path: String,
    pub filename: String,
    #[serde(rename = "size")]
    pub size_bytes: u64,
    #[serde(default)]
    pub chunk_size: Option<u64>,
    #[serde(default)]
    pub last_modified: Option<u64>,
    #[serde(default)]
    pub mime: Option<String>,
}

// Search DTOs
#[derive(Debug, Serialize)]
pub struct SearchResultDto {
    pub entry: EntryDto,
    pub version: Option<EntryVersionDto>,
    pub matches: Vec<SearchMatchDto>,
    pub score: f32,
}

impl From<vfiles_domain::SearchResult> for SearchResultDto {
    fn from(result: vfiles_domain::SearchResult) -> Self {
        Self {
            entry: result.entry.into(),
            version: result.version.map(Into::into),
            matches: result.matches.into_iter().map(Into::into).collect(),
            score: result.score,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SearchMatchDto {
    pub match_type: String,
    pub context: Option<String>,
    pub line_number: Option<u32>,
}

impl From<vfiles_domain::SearchMatch> for SearchMatchDto {
    fn from(match_: vfiles_domain::SearchMatch) -> Self {
        Self {
            match_type: match match_.match_type {
                vfiles_domain::SearchMatchType::Filename => "filename".to_string(),
                vfiles_domain::SearchMatchType::Path => "path".to_string(),
                vfiles_domain::SearchMatchType::Content => "content".to_string(),
            },
            context: match_.context,
            line_number: match_.line_number,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchQueryDto {
    pub query: String,
    pub search_files: bool,
    pub search_content: bool,
    pub limit: u32,
    pub offset: u32,
}

// Share DTOs
#[derive(Debug, Serialize)]
pub struct ShareDto {
    pub id: String,
    pub entry_id: String,
    pub code: String,
    pub expires_at: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub access_count: u32,
    pub last_accessed_at: Option<String>,
}

impl From<vfiles_domain::Share> for ShareDto {
    fn from(share: vfiles_domain::Share) -> Self {
        Self {
            id: share.id.to_string(),
            entry_id: share.entry_id.to_string(),
            code: share.code,
            expires_at: format_timestamp_opt(share.expires_at),
            created_by: share.created_by.to_string(),
            created_at: format_timestamp(share.created_at),
            access_count: share.access_count,
            last_accessed_at: format_timestamp_opt(share.last_accessed_at),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    pub path: String,
    pub expires_at: Option<String>, // ISO 8601 datetime string
}

#[derive(Debug, Serialize)]
pub struct CreateShareResponse {
    pub code: String,
    pub share_url: String,
}
