use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::ValidationError;

// ID newtypes
macro_rules! newtype_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
                Uuid::parse_str(s).map(Self)
            }

            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            pub fn to_string(&self) -> String {
                self.0.to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

newtype_id!(UserId);
newtype_id!(SessionId);
newtype_id!(NamespaceId);
newtype_id!(EntryId);
newtype_id!(VersionId);
newtype_id!(SnapshotId);
newtype_id!(UploadId);
newtype_id!(ShareId);
newtype_id!(BlobId);
newtype_id!(AccessTokenId);

// Domain entities
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub username: Username,
    pub email: Option<EmailAddress>,
    pub password_hash: String,
    pub role: Role,
    pub disabled: bool,
    pub created_at: time::OffsetDateTime,
    pub updated_at: time::OffsetDateTime,
    pub password_changed_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSession {
    pub id: SessionId,
    pub user_id: UserId,
    pub session_token_hash: String,
    pub issued_at: time::OffsetDateTime,
    pub expires_at: time::OffsetDateTime,
    pub revoked_at: Option<time::OffsetDateTime>,
    pub user_agent: Option<String>,
    pub ip_addr: Option<String>,
    pub last_seen_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    #[serde(alias = "usernameOrEmail")]
    pub username_or_email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub user: User,
    pub session: UserSession,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: UserId,
    pub username: Username,
    pub email: Option<EmailAddress>,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureMatrix {
    pub auth_enabled: bool,
    pub multi_user: bool,
    pub email_login: bool,
    pub search_content: bool,
    pub share_enabled: bool,
    pub history_enabled: bool,
    /// 是否启用 FTP(S) 批量导入（前端据此展示连接信息入口）。
    pub ftp_enabled: bool,
    /// 单文件大小上限（字节）；前端据此提示并在选择文件时先做一次校验。
    pub max_file_size_bytes: u64,
}

impl FeatureMatrix {
    pub fn capabilities_for_role(&self, is_admin: bool) -> Vec<Capability> {
        let mut caps = vec![];
        if self.auth_enabled {
            caps.push(Capability::Upload);
            caps.push(Capability::ViewHistory);
        }
        if is_admin {
            caps.push(Capability::AdminUsers);
            caps.push(Capability::Delete);
        }
        if self.share_enabled {
            caps.push(Capability::Share);
        }
        if self.email_login {
            caps.push(Capability::UseEmailLogin);
        }
        if self.search_content {
            caps.push(Capability::SearchContent);
        }
        caps
    }
}

// File system entities
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: EntryId,
    pub namespace_id: NamespaceId,
    pub parent_entry_id: Option<EntryId>,
    pub path_norm: NormalizedPath,
    pub name: String,
    pub entry_type: EntryKind,
    pub current_version_id: Option<VersionId>,
    pub created_at: time::OffsetDateTime,
    pub deleted_at: Option<time::OffsetDateTime>,
}

/// 子项 + 批量元数据（r4 ✗ 消 children N+1：一条 SQL 直取 size/mime）。
#[derive(Debug, Clone)]
pub struct EntryChildMeta {
    pub entry: Entry,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
    /// 源端 mtime（秒 ✗ rsync `-a` 快跳用；NULL = 未知）。
    pub source_mtime: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryVersion {
    pub id: VersionId,
    pub entry_id: EntryId,
    pub version_no: u32,
    pub blob_id: Option<BlobId>,
    pub size_bytes: ByteSize,
    pub mime_type: Option<String>,
    pub is_text: bool,
    pub content_hash: ContentHash,
    pub created_by: UserId,
    pub created_at: time::OffsetDateTime,
    pub change_type: ChangeType,
    pub change_message: Option<NonEmptyMessage>,
    pub source_upload_id: Option<UploadId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryVersion {
    pub id: VersionId,
    pub entry_id: EntryId,
    pub version_no: u32,
    pub created_by: UserId,
    pub created_at: time::OffsetDateTime,
    pub change_type: ChangeType,
    pub change_message: Option<NonEmptyMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: SnapshotId,
    pub namespace_id: NamespaceId,
    pub snapshot_no: u32,
    pub created_by: UserId,
    pub created_at: time::OffsetDateTime,
    pub message: Option<NonEmptyMessage>,
    pub kind: SnapshotKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotKind {
    UserCreated,
    AutoCommit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotEntry {
    pub snapshot_id: SnapshotId,
    pub entry_id: EntryId,
    pub entry_path: NormalizedPath,
    pub entry_kind: EntryKind,
    pub entry_version_id: Option<VersionId>,
    pub blob_id: Option<BlobId>,
    pub size_bytes: Option<ByteSize>,
    pub mime_type: Option<String>,
    pub version_no: Option<u32>,
    pub change_type: ChangeType,
    pub created_by: Option<UserId>,
    pub created_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blob {
    pub id: BlobId,
    pub sha256_hex: String,
    pub storage_key: String,
    pub size_bytes: ByteSize,
    pub mime_type_detected: Option<String>,
    pub ref_count: u32,
    pub created_at: time::OffsetDateTime,
    pub verified_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadSession {
    pub id: UploadId,
    pub namespace_id: NamespaceId,
    pub target_path_norm: NormalizedPath,
    pub filename: String,
    /// 客户端声明的 MIME（S3 `Content-Type` / WebDAV / HTTP 上传 ✗ 无则提交时按扩展名猜）。
    pub mime_type: Option<String>,
    pub declared_size: ByteSize,
    pub chunk_size: u64,
    pub total_chunks: u32,
    pub state: UploadState,
    pub owner_user_id: UserId,
    pub expires_at: time::OffsetDateTime,
    pub created_at: time::OffsetDateTime,
    pub updated_at: time::OffsetDateTime,
    pub completed_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadPart {
    pub upload_session_id: UploadId,
    pub part_index: u32,
    pub temp_rel_path: String,
    pub size_bytes: ByteSize,
    pub sha256_hex: String,
    pub received_at: time::OffsetDateTime,
}

/// Receipt returned after a streamed upload part has been staged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadPartReceipt {
    pub size_bytes: ByteSize,
    pub md5_hex: String,
    pub sha256_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub id: ShareId,
    pub namespace_id: NamespaceId,
    pub entry_id: EntryId,
    pub entry_version_id: Option<VersionId>,
    pub code: String,
    pub expires_at: Option<time::OffsetDateTime>,
    pub created_by: UserId,
    pub created_at: time::OffsetDateTime,
    pub access_count: u32,
    pub last_accessed_at: Option<time::OffsetDateTime>,
    pub disabled_at: Option<time::OffsetDateTime>,
}

/// 分享链接 + 被分享条目的基本信息（分享管理页需要展示文件名与位置）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareWithEntry {
    pub share: Share,
    pub entry_path: String,
    pub entry_name: String,
    pub entry_kind: EntryKind,
}

/// 审计日志条目：只追加，用于事后追溯（数据库层禁止修改/删除）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub created_at: time::OffsetDateTime,
    /// 匿名操作（如登录失败）为 `None`。
    pub user_id: Option<UserId>,
    /// 冗余的用户名快照：用户改名/删除后历史仍可读。
    pub username: String,
    /// 动作标识，例如 `login.success`、`file.upload`、`file.download`。
    pub action: String,
    pub result: AuditResult,
    /// 操作对象（路径、分享码等）。
    pub target: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// 服务端解析出的设备/浏览器描述（用于展示与筛选）。
    pub device: Option<String>,
    /// 失败原因或附加说明。
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuditResult {
    #[default]
    Success,
    Failure,
}

impl AuditResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }

    pub fn from_sql(value: &str) -> Self {
        match value {
            "failure" => Self::Failure,
            _ => Self::Success,
        }
    }
}

/// 审计日志写入参数：由各业务动作填充。
#[derive(Debug, Clone, Default)]
pub struct NewAuditLog {
    pub user_id: Option<UserId>,
    pub username: String,
    pub action: String,
    pub result: AuditResult,
    pub target: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub device: Option<String>,
    pub detail: Option<String>,
}

impl NewAuditLog {
    pub fn success(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            result: AuditResult::Success,
            ..Default::default()
        }
    }

    pub fn failure(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            result: AuditResult::Failure,
            ..Default::default()
        }
    }

    pub fn user(mut self, user_id: Option<UserId>, username: impl Into<String>) -> Self {
        self.user_id = user_id;
        self.username = username.into();
        self
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn request(mut self, ip: Option<String>, user_agent: Option<String>) -> Self {
        self.device = user_agent.as_deref().map(describe_device);
        self.ip = ip;
        self.user_agent = user_agent;
        self
    }
}

/// 把 User-Agent 归类为粗粒度描述（如「桌面 · Chrome」「移动 · Safari」）。
///
/// 只做展示与筛选所需的最小解析，不追求完整的 UA 库。
pub fn describe_device(user_agent: &str) -> String {
    let ua = user_agent.to_ascii_lowercase();
    if ua.trim().is_empty() {
        return "未知设备".to_string();
    }

    let platform = if ua.contains("iphone") || ua.contains("ipod") {
        "iPhone"
    } else if ua.contains("ipad") {
        "iPad"
    } else if ua.contains("android") {
        "Android"
    } else if ua.contains("windows") {
        "Windows"
    } else if ua.contains("macintosh") || ua.contains("mac os x") {
        "macOS"
    } else if ua.contains("linux") {
        "Linux"
    } else {
        "未知平台"
    };

    let client = if ua.contains("edg/") {
        "Edge"
    } else if ua.contains("opr/") || ua.contains("opera") {
        "Opera"
    } else if ua.contains("chrome") || ua.contains("crios") {
        "Chrome"
    } else if ua.contains("firefox") || ua.contains("fxios") {
        "Firefox"
    } else if ua.contains("safari") {
        "Safari"
    } else if ua.contains("curl") {
        "curl"
    } else if ua.contains("python") {
        "Python"
    } else {
        "未知客户端"
    };

    format!("{platform} · {client}")
}

/// 审计日志查询条件（全部可选，取交集）。
#[derive(Debug, Clone, Default)]
pub struct AuditLogQuery {
    /// 用户名或 IP 的模糊匹配。
    pub keyword: Option<String>,
    pub action: Option<String>,
    pub result: Option<AuditResult>,
    /// 起始时间（含）。
    pub since: Option<time::OffsetDateTime>,
    /// 结束时间（不含）。
    pub until: Option<time::OffsetDateTime>,
    pub limit: u32,
    pub offset: u32,
}

/// 访问令牌：给 CLI / 构建系统使用的 API 凭证。
///
/// 明文只在创建时返回一次；库里存 SHA-256 摘要与用于展示的前缀。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    pub id: AccessTokenId,
    pub user_id: UserId,
    pub name: String,
    pub token_prefix: String,
    pub scopes: String,
    pub expires_at: Option<time::OffsetDateTime>,
    pub last_used_at: Option<time::OffsetDateTime>,
    pub revoked_at: Option<time::OffsetDateTime>,
    pub created_at: time::OffsetDateTime,
}

impl AccessToken {
    /// 是否仍然可用（未撤销、未过期）。
    pub fn is_active(&self, now: time::OffsetDateTime) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        match self.expires_at {
            Some(expires_at) => expires_at > now,
            None => true,
        }
    }
}

/// 新建访问令牌所需的字段（明文由服务层生成，不入库）。
#[derive(Debug, Clone)]
pub struct NewAccessToken {
    pub id: AccessTokenId,
    pub user_id: UserId,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub scopes: String,
    pub expires_at: Option<time::OffsetDateTime>,
}

/// 审计日志查询结果。
#[derive(Debug, Clone)]
pub struct AuditLogPage {
    pub items: Vec<AuditLog>,
    pub total: u64,
}

/// 审计日志聚合项（按用户或按动作统计）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditCount {
    pub key: String,
    pub count: u64,
}

/// 审计日志概览：在**当前筛选条件**下的总量、失败数与 Top 用户/动作。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditLogSummary {
    pub total: u64,
    pub failures: u64,
    pub users: Vec<AuditCount>,
    pub actions: Vec<AuditCount>,
}

// Value objects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedPath(String);

impl NormalizedPath {
    pub fn new(path: &str) -> Result<Self, String> {
        // Allow empty string for root path
        if path.contains("..") || path.contains("//") || path.starts_with('/') {
            return Err("Invalid path format".to_string());
        }
        Ok(Self(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn new(hash: &str) -> Result<Self, String> {
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid SHA256 hash".to_string());
        }
        Ok(Self(hash.to_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailAddress(String);

impl EmailAddress {
    pub fn new(email: &str) -> Result<Self, ValidationError> {
        if !email.contains('@') {
            return Err(ValidationError::InvalidEmail(
                "Email must contain @".to_string(),
            ));
        }
        if email.len() > 254 {
            return Err(ValidationError::InvalidEmail("Email too long".to_string()));
        }
        Ok(Self(email.to_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Username(String);

impl Username {
    pub fn new(name: &str) -> Result<Self, ValidationError> {
        if name.is_empty() {
            return Err(ValidationError::InvalidUsername(
                "Username cannot be empty".to_string(),
            ));
        }
        if name.len() > 50 {
            return Err(ValidationError::InvalidUsername(
                "Username too long".to_string(),
            ));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(ValidationError::InvalidUsername(
                "Username contains invalid characters".to_string(),
            ));
        }
        Ok(Self(name.to_string()))
    }

    /// 从数据库读取时的宽容构造：**不校验**字符集。
    ///
    /// 早期版本的 bootstrap 流程可能写入了含 `-` 等字符的用户名，
    /// 这些历史数据必须仍能被列出与修复，否则整个实例都会读不出用户。
    pub fn from_stored(name: &str) -> Self {
        Self(name.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonEmptyMessage(String);

impl NonEmptyMessage {
    pub fn new(msg: &str) -> Result<Self, String> {
        if msg.trim().is_empty() {
            return Err("Message cannot be empty".to_string());
        }
        Ok(Self(msg.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteSize(u64);

impl ByteSize {
    pub fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

// Enums
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Admin,
    Manager,
    User,
}

impl Role {
    pub fn can_access_admin_panel(self) -> bool {
        matches!(self, Self::Admin | Self::Manager)
    }

    pub fn is_admin(self) -> bool {
        matches!(self, Self::Admin)
    }

    pub fn is_manager(self) -> bool {
        matches!(self, Self::Manager)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Admin => write!(f, "admin"),
            Role::Manager => write!(f, "manager"),
            Role::User => write!(f, "user"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadState {
    Receiving,
    Assembling,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    Upload,
    Delete,
    Share,
    AdminUsers,
    UseEmailLogin,
    ViewHistory,
    SearchContent,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Capability::Upload => write!(f, "upload"),
            Capability::Delete => write!(f, "delete"),
            Capability::Share => write!(f, "share"),
            Capability::AdminUsers => write!(f, "admin_users"),
            Capability::UseEmailLogin => write!(f, "use_email_login"),
            Capability::ViewHistory => write!(f, "view_history"),
            Capability::SearchContent => write!(f, "search_content"),
        }
    }
}

// Search types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub namespace_id: NamespaceId,
    pub search_files: bool,
    pub search_content: bool,
    pub path_prefix: Option<NormalizedPath>,
    pub entry_kind: Option<EntryKind>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub entry: Entry,
    pub version: Option<EntryVersion>,
    pub matches: Vec<SearchMatch>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub match_type: SearchMatchType,
    pub context: Option<String>,
    pub line_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMatchType {
    Filename,
    Path,
    Content,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalized_path() {
        assert!(NormalizedPath::new("").is_ok()); // Empty string is allowed for root path
        assert!(NormalizedPath::new("../test").is_err());
        assert!(NormalizedPath::new("valid/path").is_ok());
    }

    #[test]
    fn test_content_hash() {
        assert!(ContentHash::new("invalid").is_err());
        assert!(
            ContentHash::new("a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3")
                .is_ok()
        );
    }

    #[test]
    fn test_email_address() {
        assert!(EmailAddress::new("invalid").is_err());
        assert!(EmailAddress::new("test@example.com").is_ok());
    }

    #[test]
    fn test_username() {
        assert!(Username::new("").is_err());
        assert!(Username::new("valid_user").is_ok());
        assert!(Username::new("invalid-user").is_err());
    }

    #[test]
    fn test_user_id() {
        let id = UserId::new();
        assert!(!id.to_string().is_empty());
    }
}
