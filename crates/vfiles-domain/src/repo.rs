use crate::*;

/// Object-safe blob stream that supports seeking for HTTP Range requests.
pub trait ReadSeek: tokio::io::AsyncRead + tokio::io::AsyncSeek {}

impl<T: tokio::io::AsyncRead + tokio::io::AsyncSeek + ?Sized> ReadSeek for T {}

// Repo traits
#[async_trait::async_trait]
pub trait UserRepo {
    async fn create_admin(
        &self,
        username: &str,
        email: &str,
        password_hash: &str,
    ) -> DomainResult<UserId>;
    async fn count_admins(&self) -> DomainResult<i64>;
    async fn create_user(
        &self,
        username: &Username,
        email: Option<&EmailAddress>,
        password_hash: &str,
        role: Role,
    ) -> DomainResult<UserId>;
    async fn find_by_username(&self, username: &Username) -> DomainResult<User>;
    async fn find_by_email(&self, email: &EmailAddress) -> DomainResult<User>;
    async fn find_by_id(&self, id: &UserId) -> DomainResult<User>;
    async fn update_email(&self, id: &UserId, email: &EmailAddress) -> DomainResult<()>;
    async fn update_password(&self, id: &UserId, password_hash: &str) -> DomainResult<()>;
    async fn disable_user(&self, id: &UserId) -> DomainResult<()>;
    async fn list_users(&self, limit: i64, offset: i64) -> DomainResult<Vec<User>>;
}

#[async_trait::async_trait]
pub trait SessionRepo {
    async fn create_session(
        &self,
        user_id: &UserId,
        session_token_hash: &str,
        expires_at: time::OffsetDateTime,
        user_agent: Option<&str>,
        ip_addr: Option<&str>,
    ) -> DomainResult<SessionId>;
    async fn find_session_by_token_hash(&self, token_hash: &str) -> DomainResult<UserSession>;
    async fn update_last_seen(&self, id: &SessionId) -> DomainResult<()>;
    async fn revoke_session(&self, id: &SessionId) -> DomainResult<()>;
    async fn revoke_user_sessions(&self, user_id: &UserId) -> DomainResult<()>;
    async fn cleanup_expired_sessions(&self) -> DomainResult<i64>;
}

/// 命名空间统计（侧栏聚合入口使用）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NamespaceStats {
    pub file_count: u64,
    pub directory_count: u64,
    pub total_bytes: u64,
}

/// 按类型聚合的占用统计，用于侧栏「存储用量」的占比条。
///
/// 分类依据是当前版本的 MIME 类型（缺失时归入 `Other`），与主流网盘的
/// 「文档 / 图片 / 视频 / 音频 / 其它」分组保持一致。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileCategory {
    Document,
    Image,
    Video,
    Audio,
    Other,
}

impl FileCategory {
    /// 稳定标识：用于 API 字段与前端样式类名。
    pub fn as_str(self) -> &'static str {
        match self {
            FileCategory::Document => "document",
            FileCategory::Image => "image",
            FileCategory::Video => "video",
            FileCategory::Audio => "audio",
            FileCategory::Other => "other",
        }
    }

    /// 从数据库返回的分类名解析（未知值归入 `Other`）。
    pub fn from_sql(value: &str) -> Self {
        match value {
            "document" => FileCategory::Document,
            "image" => FileCategory::Image,
            "video" => FileCategory::Video,
            "audio" => FileCategory::Audio,
            _ => FileCategory::Other,
        }
    }
}

/// 单个分类的占用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryUsage {
    pub category: FileCategory,
    pub bytes: u64,
    pub file_count: u64,
}

#[async_trait::async_trait]
pub trait NamespaceRepo {
    async fn create_default(&self, owner_id: &UserId, slug: &str) -> DomainResult<NamespaceId>;
    async fn find_default(&self) -> DomainResult<NamespaceId>;
    async fn find_default_for_owner(&self, owner_id: &UserId) -> DomainResult<NamespaceId>;
}

#[async_trait::async_trait]
pub trait SystemSettingsRepo {
    async fn set_bootstrapped(&self) -> DomainResult<()>;
    async fn is_bootstrapped(&self) -> DomainResult<bool>;
}

#[async_trait::async_trait]
pub trait EntryRepo {
    async fn find_by_id(&self, entry_id: &EntryId) -> DomainResult<Entry>;
    async fn find_by_path(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> DomainResult<Option<Entry>>;
    async fn find_children(
        &self,
        namespace_id: &NamespaceId,
        parent_path: &NormalizedPath,
    ) -> DomainResult<Vec<Entry>>;
    /// 一次取回命名空间下的全部条目（含 current_version_id），避免递归列举。
    async fn find_all(&self, namespace_id: &NamespaceId) -> DomainResult<Vec<Entry>>;

    /// 命名空间内的条目统计（文件数、目录数、总字节数），用 SQL 聚合避免全量拉取。
    async fn stats(&self, namespace_id: &NamespaceId) -> DomainResult<NamespaceStats>;

    /// 按文件类型聚合的占用（只统计文件、只算当前版本）。
    async fn stats_by_category(
        &self,
        namespace_id: &NamespaceId,
    ) -> DomainResult<Vec<CategoryUsage>>;

    /// 最近更新的文件（按版本创建时间倒序），用于侧栏「最近」。
    async fn recent_files(
        &self,
        namespace_id: &NamespaceId,
        limit: u32,
    ) -> DomainResult<Vec<Entry>>;
    /// 一次取回某目录及其所有后代（含 root 自身），按路径排序。
    /// 分页取直接子条目：按「目录优先 + 路径升序」排序（与应用层排序一致），
    /// 同时返回该目录下的条目总数，避免把整棵目录拉进内存后再切片。
    async fn find_children_page(
        &self,
        namespace_id: &NamespaceId,
        parent_path: &NormalizedPath,
        limit: u32,
        offset: u32,
    ) -> DomainResult<(Vec<Entry>, u64)>;

    async fn find_subtree(
        &self,
        namespace_id: &NamespaceId,
        root_path: &NormalizedPath,
    ) -> DomainResult<Vec<Entry>>;
    /// 批量按路径查询（用于移动前的冲突检查），只返回存在的条目。
    async fn find_paths(
        &self,
        namespace_id: &NamespaceId,
        paths: &[NormalizedPath],
    ) -> DomainResult<Vec<Entry>>;
    /// 列出所有仍被版本或快照引用的 blob id（维护任务使用）。
    async fn referenced_blob_ids(&self) -> DomainResult<Vec<BlobId>>;
    async fn create_entry(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        kind: EntryKind,
        user_id: &UserId,
    ) -> DomainResult<EntryId>;
    async fn update_current_version(
        &self,
        entry_id: &EntryId,
        version_id: &VersionId,
    ) -> DomainResult<()>;
    async fn delete_entry(&self, entry_id: &EntryId) -> DomainResult<()>;
    /// 批量删除条目，避免逐个删除造成 N 次查询。
    async fn delete_entries(&self, entry_ids: &[EntryId]) -> DomainResult<()>;
    async fn release_blob_references(
        &self,
        references: &[(BlobId, u32)],
    ) -> DomainResult<Vec<BlobId>>;
    async fn move_entry(&self, entry_id: &EntryId, new_path: &NormalizedPath) -> DomainResult<()>;
    /// 批量更新路径，在单个事务内执行，避免逐个提交。
    async fn move_entries(&self, moves: &[(EntryId, NormalizedPath)]) -> DomainResult<()>;
    async fn get_entry_history(
        &self,
        entry_id: &EntryId,
        limit: u32,
        cursor: Option<&str>,
    ) -> DomainResult<Vec<EntryVersion>>;
    async fn find_version(&self, version_id: &VersionId) -> DomainResult<EntryVersion>;
    /// 批量获取版本，避免目录列举时的 N+1 查询；不存在的 id 会被跳过。
    async fn find_versions(&self, version_ids: &[VersionId]) -> DomainResult<Vec<EntryVersion>>;
    /// 批量获取多条条目的全部版本（用于删除前的引用计数汇总）。
    async fn find_versions_for_entries(
        &self,
        entry_ids: &[EntryId],
    ) -> DomainResult<Vec<EntryVersion>>;
    #[allow(clippy::too_many_arguments)]
    async fn create_version(
        &self,
        entry_id: &EntryId,
        blob_id: Option<&BlobId>,
        content_hash: Option<&ContentHash>,
        size_bytes: u64,
        mime_type: Option<&str>,
        created_by: &UserId,
        message: Option<&str>,
    ) -> DomainResult<EntryVersion>;
}

/// 收藏夹：以条目 ID 记录，重命名/移动后依然有效。
#[async_trait::async_trait]
pub trait FavoriteRepo {
    /// 列出收藏的条目（按收藏时间倒序）。
    async fn list(&self, namespace_id: &NamespaceId) -> DomainResult<Vec<Entry>>;

    /// 添加收藏；已存在时幂等返回 `false`。
    async fn add(&self, namespace_id: &NamespaceId, entry_id: &EntryId) -> DomainResult<bool>;

    /// 取消收藏；不存在时返回 `false`。
    async fn remove(&self, namespace_id: &NamespaceId, entry_id: &EntryId) -> DomainResult<bool>;

    /// 该条目是否已收藏。
    async fn contains(&self, namespace_id: &NamespaceId, entry_id: &EntryId) -> DomainResult<bool>;
}

#[async_trait::async_trait]
pub trait SnapshotRepo {
    async fn create_snapshot(
        &self,
        namespace_id: &NamespaceId,
        message: Option<&str>,
        kind: SnapshotKind,
        user_id: &UserId,
    ) -> DomainResult<SnapshotId>;
    async fn find_snapshot(&self, snapshot_id: &SnapshotId) -> DomainResult<Snapshot>;
    async fn list_snapshots(
        &self,
        namespace_id: &NamespaceId,
        limit: u32,
        cursor: Option<&str>,
    ) -> DomainResult<Vec<Snapshot>>;
    async fn add_snapshot_entries(
        &self,
        snapshot_id: &SnapshotId,
        entries: &[SnapshotEntry],
    ) -> DomainResult<()>;
    async fn get_snapshot_entries(
        &self,
        snapshot_id: &SnapshotId,
    ) -> DomainResult<Vec<SnapshotEntry>>;
    /// 列出全部快照（按创建时间倒序，维护任务使用）。
    async fn list_all_snapshots(&self) -> DomainResult<Vec<Snapshot>>;
    /// 删除快照及其条目（条目通过外键级联删除）。
    async fn delete_snapshot(&self, snapshot_id: &SnapshotId) -> DomainResult<()>;
}

#[async_trait::async_trait]
pub trait BlobStore {
    async fn store_blob(
        &self,
        data: &[u8],
        expected_sha256: Option<&str>,
    ) -> DomainResult<(BlobId, ContentHash, bool)>;
    async fn store_blob_stream(
        &self,
        reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        expected_sha256: Option<&str>,
    ) -> DomainResult<(BlobId, ContentHash, bool, u64)>;
    async fn get_blob(&self, blob_id: &BlobId) -> DomainResult<Option<Vec<u8>>>;
    async fn get_blob_stream(
        &self,
        blob_id: &BlobId,
    ) -> DomainResult<Option<Box<dyn ReadSeek + Send + Unpin>>>;
    async fn delete_blob(&self, blob_id: &BlobId) -> DomainResult<()>;
    async fn blob_exists(&self, sha256: &ContentHash) -> DomainResult<bool>;
    async fn get_blob_metadata(&self, blob_id: &BlobId) -> DomainResult<Option<Blob>>;
    /// 列出全部 blob 元数据（维护任务使用）。
    async fn list_blobs(&self) -> DomainResult<Vec<Blob>>;
    /// 删除未被引用的 blob（同时删除数据库行与文件）。
    ///
    /// 通过 `expected_ref_count` 做乐观校验：若期间计数被并发修改则跳过删除。
    async fn purge_blob(&self, blob_id: &BlobId, expected_ref_count: u32) -> DomainResult<bool>;
    /// 列出磁盘上实际存在的 blob 文件及其修改时间（维护任务使用）。
    ///
    /// 用于发现「文件已写入但数据库行未建立」的孤兒文件（例如上传中断）。
    async fn list_stored_blob_files(
        &self,
    ) -> DomainResult<Vec<(BlobId, time::OffsetDateTime, u64)>>;
}

#[async_trait::async_trait]
pub trait UploadStore {
    async fn create_upload_session(
        &self,
        namespace_id: &NamespaceId,
        target_path: &NormalizedPath,
        filename: &str,
        size_bytes: u64,
        chunk_size: u64,
        user_id: &UserId,
    ) -> DomainResult<UploadId>;
    async fn get_upload_session(&self, upload_id: &UploadId) -> DomainResult<UploadSession>;
    async fn store_upload_part(
        &self,
        upload_id: &UploadId,
        part_index: u32,
        data: &[u8],
        sha256: &str,
    ) -> DomainResult<()>;
    async fn get_upload_parts(&self, upload_id: &UploadId) -> DomainResult<Vec<UploadPart>>;
    async fn assemble_upload_stream(
        &self,
        upload_id: &UploadId,
    ) -> DomainResult<Box<dyn tokio::io::AsyncRead + Send + Unpin>>;
    async fn assemble_upload(&self, upload_id: &UploadId) -> DomainResult<Vec<u8>>;
    async fn complete_upload_session(&self, upload_id: &UploadId) -> DomainResult<()>;
    async fn cancel_upload_session(&self, upload_id: &UploadId) -> DomainResult<()>;
    async fn cleanup_expired_sessions(&self) -> DomainResult<i64>;
}

#[async_trait::async_trait]
pub trait SearchRepo {
    async fn search_entries(&self, query: &SearchQuery) -> DomainResult<Vec<SearchResult>>;
    async fn search_content(&self, query: &SearchQuery) -> DomainResult<Vec<SearchResult>>;
}

#[async_trait::async_trait]
pub trait ShareRepo {
    async fn create_share(
        &self,
        namespace_id: &NamespaceId,
        entry_id: &EntryId,
        entry_version_id: Option<&VersionId>,
        code: &str,
        expires_at: Option<time::OffsetDateTime>,
        created_by: &UserId,
    ) -> DomainResult<ShareId>;
    async fn find_share_by_code(&self, code: &str) -> DomainResult<Share>;
    async fn find_shares_by_entry(&self, entry_id: &EntryId) -> DomainResult<Vec<Share>>;
    async fn find_shares_by_user(&self, user_id: &UserId) -> DomainResult<Vec<Share>>;
    async fn record_share_access(&self, share_id: &ShareId) -> DomainResult<()>;
    async fn disable_share(&self, share_id: &ShareId) -> DomainResult<()>;
    async fn cleanup_expired_shares(&self) -> DomainResult<i64>;
}

#[async_trait::async_trait]
pub trait AdminRepo {
    async fn list_users(&self, limit: i64, offset: i64) -> DomainResult<Vec<User>>;
    async fn count_users(&self) -> DomainResult<i64>;
    async fn create_user(
        &self,
        username: &Username,
        email: &EmailAddress,
        password_hash: &str,
        role: Role,
    ) -> DomainResult<UserId>;
    async fn update_user_role(&self, user_id: &UserId, role: Role) -> DomainResult<()>;
    async fn disable_user(&self, user_id: &UserId) -> DomainResult<()>;
    async fn enable_user(&self, user_id: &UserId) -> DomainResult<()>;
    async fn delete_user(&self, user_id: &UserId) -> DomainResult<()>;
    async fn reset_user_password(
        &self,
        user_id: &UserId,
        new_password_hash: &str,
    ) -> DomainResult<()>;
}
