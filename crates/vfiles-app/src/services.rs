use std::{collections::HashMap, io::Write, sync::Arc};

use tokio::io::AsyncReadExt;
use vfiles_domain::*;
use vfiles_infra_sqlite::{
    FsBlobStore, FsUploadStore, SqliteEntryRepo, SqliteSessionRepo, SqliteSnapshotRepo,
    SqliteUserRepo,
};

#[derive(Clone, Debug, Default)]
pub struct CopyOptions {
    pub overwrite: bool,
    pub depth_infinity: bool,
    pub condition: Option<EntryWriteCondition>,
    pub destination_lock_states: Vec<EntryLockSnapshot>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MoveOptions<'a> {
    pub destination_is_container: bool,
    pub overwrite_destination: bool,
    pub property_changes: Option<&'a [EntryPropertyChange]>,
    pub condition: Option<&'a EntryWriteCondition>,
}

pub(crate) fn normalize_message(message: Option<&str>) -> Option<String> {
    message.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(crate) fn validate_filename(filename: &str) -> DomainResult<()> {
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename == "."
        || filename == ".."
    {
        return Err(DomainError::Validation {
            message: "Invalid filename".to_string(),
        });
    }

    Ok(())
}

pub(crate) fn uploaded_file_path(
    target_path: &NormalizedPath,
    filename: &str,
) -> DomainResult<NormalizedPath> {
    validate_filename(filename)?;

    let full_path = if target_path.as_str().is_empty() {
        filename.to_string()
    } else {
        format!(
            "{}/{}",
            target_path.as_str().trim_end_matches('/'),
            filename
        )
    };

    NormalizedPath::new(&full_path).map_err(|_| DomainError::Validation {
        message: "Invalid uploaded file path".to_string(),
    })
}

fn preview_kind(mime_type: Option<&str>) -> Option<String> {
    match mime_type {
        Some(value) if value.starts_with("text/") => Some("text".to_string()),
        Some(value) if value.starts_with("image/") => Some("image".to_string()),
        Some("application/pdf") => Some("pdf".to_string()),
        Some(value)
            if value == "application/json"
                || value == "application/xml"
                || value.ends_with("+json")
                || value.ends_with("+xml") =>
        {
            Some("text".to_string())
        }
        Some(_) => Some("binary".to_string()),
        None => None,
    }
}

pub(crate) fn guess_mime_type(filename: &str) -> Option<String> {
    mime_guess::from_path(filename)
        .first_raw()
        .map(|value| value.to_string())
}

fn actor_name_for(version: &EntryVersion) -> String {
    version.created_by.to_string()
}

fn message_for(version: &EntryVersion) -> Option<String> {
    version
        .change_message
        .as_ref()
        .map(|value| value.as_str().to_string())
}

fn is_text_mime_type(mime_type: Option<&str>) -> bool {
    match mime_type {
        Some(value) if value.starts_with("text/") => true,
        Some(value)
            if value == "application/json"
                || value == "application/xml"
                || value.ends_with("+json")
                || value.ends_with("+xml") =>
        {
            true
        }
        _ => false,
    }
}

fn invalid_commit_error() -> DomainError {
    DomainError::Validation {
        message: "Invalid commit/version ID format".to_string(),
    }
}

fn synthetic_snapshot_entry_id(snapshot_id: &SnapshotId, path: &NormalizedPath) -> EntryId {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(snapshot_id.to_string().as_bytes());
    hasher.update(path.as_str().as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    EntryId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

#[derive(Debug, Clone)]
pub(crate) struct PendingSnapshotEntry {
    entry_id: EntryId,
    entry_path: NormalizedPath,
    entry_kind: EntryKind,
    entry_version_id: Option<VersionId>,
    blob_id: Option<BlobId>,
    size_bytes: Option<ByteSize>,
    mime_type: Option<String>,
    version_no: Option<u32>,
    change_type: ChangeType,
    created_by: Option<UserId>,
    created_at: Option<time::OffsetDateTime>,
}

impl PendingSnapshotEntry {
    fn into_snapshot_entry(
        self,
        snapshot_id: SnapshotId,
        default_actor: &UserId,
        default_at: time::OffsetDateTime,
    ) -> SnapshotEntry {
        SnapshotEntry {
            snapshot_id,
            entry_id: self.entry_id,
            entry_path: self.entry_path,
            entry_kind: self.entry_kind,
            entry_version_id: self.entry_version_id,
            blob_id: self.blob_id,
            size_bytes: self.size_bytes,
            mime_type: self.mime_type,
            version_no: self.version_no,
            change_type: self.change_type,
            created_by: self.created_by.or(Some(*default_actor)),
            created_at: self.created_at.or(Some(default_at)),
        }
    }
}

fn pending_snapshot_entry(
    entry_id: EntryId,
    entry_path: &NormalizedPath,
    entry_kind: EntryKind,
    version: Option<&EntryVersion>,
    change_type: ChangeType,
) -> PendingSnapshotEntry {
    let retain_version = change_type != ChangeType::Deleted;

    PendingSnapshotEntry {
        entry_id,
        entry_path: entry_path.clone(),
        entry_kind,
        entry_version_id: retain_version
            .then(|| version.map(|value| value.id))
            .flatten(),
        blob_id: retain_version
            .then(|| version.and_then(|value| value.blob_id))
            .flatten(),
        size_bytes: retain_version
            .then(|| version.map(|value| value.size_bytes))
            .flatten(),
        mime_type: retain_version
            .then(|| version.and_then(|value| value.mime_type.clone()))
            .flatten(),
        version_no: retain_version
            .then(|| version.map(|value| value.version_no))
            .flatten(),
        change_type,
        created_by: version.map(|value| value.created_by),
        created_at: version.map(|value| value.created_at),
    }
}

fn tree_item_from_snapshot(snapshot: &Snapshot, snapshot_entry: &SnapshotEntry) -> TreeItem {
    let modified_at = snapshot_entry.created_at.or(Some(snapshot.created_at));

    TreeItem {
        entry_id: snapshot_entry.entry_id,
        name: basename(&snapshot_entry.entry_path).to_string(),
        path: snapshot_entry.entry_path.as_str().to_string(),
        kind: snapshot_entry.entry_kind,
        size_bytes: snapshot_entry.size_bytes.map(|value| value.as_u64()),
        created_at: modified_at.unwrap_or(snapshot.created_at),
        modified_at,
        version_id: snapshot_entry.entry_version_id,
        mime_type: snapshot_entry.mime_type.clone(),
        is_text: snapshot_entry
            .mime_type
            .as_deref()
            .map(|value| is_text_mime_type(Some(value))),
        preview_kind: preview_kind(snapshot_entry.mime_type.as_deref()),
        last_change: modified_at.map(|at| ChangeSummary {
            actor_name: snapshot_entry
                .created_by
                .map(|value| value.to_string())
                .unwrap_or_else(|| snapshot.created_by.to_string()),
            message: None,
            at,
            snapshot_id: Some(snapshot.id),
            version_id: snapshot_entry.entry_version_id,
        }),
    }
}

fn path_depth(path: &NormalizedPath) -> usize {
    if path.as_str().is_empty() {
        0
    } else {
        path.as_str().split('/').count()
    }
}

fn basename(path: &NormalizedPath) -> &str {
    path.as_str().rsplit('/').next().unwrap_or(path.as_str())
}

fn archive_name_for(path: &NormalizedPath) -> String {
    path.as_str()
        .split('/')
        .rfind(|segment| !segment.is_empty())
        .unwrap_or("root")
        .to_string()
}

fn relative_path_for_directory(
    requested_path: &NormalizedPath,
    full_path: &NormalizedPath,
) -> String {
    if requested_path.as_str().is_empty() {
        return full_path.as_str().to_string();
    }

    if full_path.as_str() == requested_path.as_str() {
        return full_path
            .as_str()
            .split('/')
            .next_back()
            .unwrap_or(full_path.as_str())
            .to_string();
    }

    full_path
        .as_str()
        .strip_prefix(requested_path.as_str())
        .unwrap_or(full_path.as_str())
        .trim_start_matches('/')
        .to_string()
}

fn breadcrumbs_for(path: &NormalizedPath) -> Vec<BreadcrumbItem> {
    let mut breadcrumbs = vec![BreadcrumbItem {
        name: "root".to_string(),
        path: String::new(),
    }];

    if path.as_str().is_empty() {
        return breadcrumbs;
    }

    let mut current = String::new();
    for segment in path.as_str().split('/') {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        breadcrumbs.push(BreadcrumbItem {
            name: segment.to_string(),
            path: current.clone(),
        });
    }

    breadcrumbs
}

fn join_path(parent: &NormalizedPath, child: &str) -> DomainResult<NormalizedPath> {
    let joined = if parent.as_str().is_empty() {
        child.to_string()
    } else {
        format!("{}/{}", parent.as_str().trim_end_matches('/'), child)
    };

    NormalizedPath::new(&joined).map_err(|_| DomainError::Validation {
        message: format!("Invalid path: {}", joined),
    })
}

fn compute_move_target(
    source_root: &NormalizedPath,
    entry_path: &NormalizedPath,
    target_root: &NormalizedPath,
) -> DomainResult<NormalizedPath> {
    if entry_path == source_root {
        return Ok(target_root.clone());
    }

    let suffix = entry_path
        .as_str()
        .strip_prefix(source_root.as_str())
        .unwrap_or(entry_path.as_str())
        .trim_start_matches('/');

    join_path(target_root, suffix)
}

fn upload_view_from_session(upload: &UploadSession, parts: &[UploadPart]) -> UploadSessionView {
    let received_parts = parts.iter().map(|part| part.part_index).collect::<Vec<_>>();
    let missing_parts = (0..upload.total_chunks)
        .filter(|index| !received_parts.contains(index))
        .collect::<Vec<_>>();

    UploadSessionView {
        upload_id: upload.id,
        target_path: upload.target_path_norm.as_str().to_string(),
        chunk_size: upload.chunk_size,
        total_parts: upload.total_chunks,
        received_parts,
        missing_parts,
        state: upload.state,
        expires_at: upload.expires_at,
    }
}

async fn collect_descendants(
    entry_repo: &(dyn EntryRepo + Send + Sync),
    namespace_id: &NamespaceId,
    root: &Entry,
) -> DomainResult<Vec<Entry>> {
    // 单次范围查询取回 root 及其全部后代，避免按目录递归（O(目录数) 次查询）。
    entry_repo.find_subtree(namespace_id, &root.path_norm).await
}

async fn collect_namespace_entries(
    entry_repo: &(dyn EntryRepo + Send + Sync),
    namespace_id: &NamespaceId,
) -> DomainResult<Vec<Entry>> {
    // 单次查询取回全部条目，避免按目录递归列举（O(目录数) 次查询）。
    let mut entries = entry_repo.find_all(namespace_id).await?;
    entries.sort_by(|left, right| left.path_norm.as_str().cmp(right.path_norm.as_str()));
    Ok(entries)
}

fn snapshot_change_type(entry_kind: EntryKind, version: Option<&EntryVersion>) -> ChangeType {
    version
        .map(|value| value.change_type)
        .unwrap_or(match entry_kind {
            EntryKind::Directory => ChangeType::Added,
            EntryKind::File => ChangeType::Added,
        })
}

pub(crate) async fn collect_snapshot_state(
    entry_repo: &(dyn EntryRepo + Send + Sync),
    namespace_id: &NamespaceId,
    extra_entries: Vec<PendingSnapshotEntry>,
) -> DomainResult<Vec<PendingSnapshotEntry>> {
    let entries = collect_namespace_entries(entry_repo, namespace_id).await?;

    // 批量取当前版本，避免每个条目一次查询（每次变更都会走这里）。
    let version_ids: Vec<VersionId> = entries
        .iter()
        .filter_map(|entry| match (entry.entry_type, entry.current_version_id) {
            (EntryKind::File, Some(version_id)) => Some(version_id),
            _ => None,
        })
        .collect();
    let versions_by_id: HashMap<VersionId, EntryVersion> = entry_repo
        .find_versions(&version_ids)
        .await?
        .into_iter()
        .map(|version| (version.id, version))
        .collect();

    let mut snapshot_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        let version = match (entry.entry_type, entry.current_version_id) {
            (EntryKind::File, Some(version_id)) => versions_by_id.get(&version_id),
            _ => None,
        };
        snapshot_entries.push(pending_snapshot_entry(
            entry.id,
            &entry.path_norm,
            entry.entry_type,
            version,
            snapshot_change_type(entry.entry_type, version),
        ));
    }

    snapshot_entries.extend(extra_entries);
    snapshot_entries.sort_by(|left, right| left.entry_path.as_str().cmp(right.entry_path.as_str()));
    Ok(snapshot_entries)
}

pub(crate) async fn ensure_directory_path(
    entry_repo: &(dyn EntryRepo + Send + Sync),
    namespace_id: &NamespaceId,
    directory_path: &NormalizedPath,
    user_id: &UserId,
) -> DomainResult<Vec<ChangedEntry>> {
    if directory_path.as_str().is_empty() {
        return Ok(Vec::new());
    }

    let mut current = String::new();
    let mut changed_entries = Vec::new();

    for segment in directory_path.as_str().split('/') {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);

        let current_path = NormalizedPath::new(&current).map_err(|_| DomainError::Validation {
            message: format!("Invalid path: {}", current),
        })?;

        if let Some(entry) = entry_repo.find_by_path(namespace_id, &current_path).await? {
            if entry.entry_type != EntryKind::Directory {
                return Err(DomainError::PathConflict {
                    message: format!("Path is occupied by a file: {}", current_path.as_str()),
                });
            }
            continue;
        }

        let entry_id = entry_repo
            .create_entry(namespace_id, &current_path, EntryKind::Directory, user_id)
            .await?;
        changed_entries.push(ChangedEntry {
            entry_id,
            path: current.clone(),
            kind: EntryKind::Directory,
            current_version_id: None,
            change_type: ChangeType::Added,
        });
    }

    Ok(changed_entries)
}

pub(crate) async fn create_snapshot_record(
    snapshot_repo: &(dyn SnapshotRepo + Send + Sync),
    namespace_id: &NamespaceId,
    message: Option<&str>,
    kind: SnapshotKind,
    user_id: &UserId,
    snapshot_entries: Vec<PendingSnapshotEntry>,
) -> DomainResult<SnapshotId> {
    let normalized_message = normalize_message(message);
    let applied_at = time::OffsetDateTime::now_utc();
    let snapshot_id = snapshot_repo
        .create_snapshot(namespace_id, normalized_message.as_deref(), kind, user_id)
        .await?;

    if !snapshot_entries.is_empty() {
        let snapshot_entries = snapshot_entries
            .into_iter()
            .map(|entry| entry.into_snapshot_entry(snapshot_id, user_id, applied_at))
            .collect::<Vec<_>>();
        snapshot_repo
            .add_snapshot_entries(&snapshot_id, &snapshot_entries)
            .await?;
    }

    Ok(snapshot_id)
}

fn path_matches_scope(path: &NormalizedPath, scope: &NormalizedPath) -> bool {
    scope.as_str().is_empty()
        || path == scope
        || path
            .as_str()
            .starts_with(&format!("{}/", scope.as_str().trim_end_matches('/')))
}

fn validate_snapshot_directory_scope(
    path: &NormalizedPath,
    snapshot_entries: &[SnapshotEntry],
) -> DomainResult<()> {
    if path.as_str().is_empty() {
        return Ok(());
    }
    let mut visible_entries = snapshot_entries
        .iter()
        .filter(|entry| entry.change_type != ChangeType::Deleted);
    if let Some(entry) = visible_entries
        .clone()
        .find(|entry| entry.entry_path == *path)
    {
        if entry.entry_kind != EntryKind::Directory {
            return Err(DomainError::Validation {
                message: "Path is not a directory".to_string(),
            });
        }
    } else {
        let prefix = format!("{}/", path.as_str());
        let has_descendants =
            visible_entries.any(|entry| entry.entry_path.as_str().starts_with(&prefix));
        if !has_descendants {
            return Err(DomainError::NotFound {
                resource: "snapshot entry".to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) async fn finalize_mutation(
    snapshot_repo: &(dyn SnapshotRepo + Send + Sync),
    namespace_id: &NamespaceId,
    message: Option<&str>,
    user_id: &UserId,
    changed_entries: Vec<ChangedEntry>,
    snapshot_entries: Vec<PendingSnapshotEntry>,
    warnings: Vec<String>,
) -> DomainResult<MutationResult> {
    let applied_at = time::OffsetDateTime::now_utc();
    let snapshot_id = create_snapshot_record(
        snapshot_repo,
        namespace_id,
        message,
        SnapshotKind::AutoCommit,
        user_id,
        snapshot_entries,
    )
    .await?;

    Ok(MutationResult {
        snapshot_id,
        changed_entries,
        warnings,
        applied_at,
    })
}

/// 生产环境使用的具体仓储组合。
///
/// 交付层（HTTP、FTP）需要按同一组合装配 `WorkspaceService`，这里给出别名，
/// 避免各处重复书写泛型参数。
pub type DefaultWorkspaceService =
    WorkspaceService<SqliteEntryRepo, SqliteSnapshotRepo, FsBlobStore, FsUploadStore>;

/// 同上，用于上传服务。
pub type DefaultUploadService =
    UploadService<SqliteEntryRepo, SqliteSnapshotRepo, FsBlobStore, FsUploadStore>;

/// 命名空间解析：按用户取默认命名空间，不存在则创建。
///
/// HTTP（多用户模式）与 FTP 认证后都需要这段逻辑，故放在应用层共用；
/// 完整逻辑见 `ensure_default_namespace`。
#[derive(Clone)]
pub struct NamespaceService {
    namespace_repo: Arc<dyn NamespaceRepo + Send + Sync>,
}

impl std::fmt::Debug for NamespaceService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamespaceService").finish_non_exhaustive()
    }
}

impl NamespaceService {
    pub fn new(namespace_repo: Arc<dyn NamespaceRepo + Send + Sync>) -> Self {
        Self { namespace_repo }
    }

    /// 取该用户的默认命名空间；不存在时创建（并发创建视为成功）。
    pub async fn ensure_default_for_owner(&self, owner_id: &UserId) -> DomainResult<NamespaceId> {
        match self.namespace_repo.find_default_for_owner(owner_id).await {
            Ok(namespace_id) => Ok(namespace_id),
            Err(DomainError::NotFound { .. }) => {
                match self
                    .namespace_repo
                    .create_default(owner_id, "default")
                    .await
                {
                    Ok(namespace_id) => Ok(namespace_id),
                    // 并发下另一个请求可能已经创建成功
                    Err(DomainError::Conflict { .. }) => {
                        self.namespace_repo.find_default_for_owner(owner_id).await
                    }
                    Err(err) => Err(err),
                }
            }
            Err(err) => Err(err),
        }
    }
}

// Services
#[derive(Debug)]
pub struct BootstrapService<R, N, S> {
    user_repo: R,
    namespace_repo: N,
    settings_repo: S,
}

impl<R, N, S> BootstrapService<R, N, S>
where
    R: UserRepo,
    N: NamespaceRepo,
    S: SystemSettingsRepo,
{
    pub fn new(user_repo: R, namespace_repo: N, settings_repo: S) -> Self {
        Self {
            user_repo,
            namespace_repo,
            settings_repo,
        }
    }

    pub async fn bootstrap_admin(
        &self,
        username: &str,
        email: &str,
        password_hash: &str,
    ) -> DomainResult<UserId> {
        let admin_count = self.user_repo.count_admins().await?;
        if admin_count > 0 {
            return Err(DomainError::Conflict {
                message: "Admin already exists".to_string(),
            });
        }

        // 与普通创建一致地校验用户名：否则会写入读不出来的账号，实例直接不可用
        Username::new(username)?;
        EmailAddress::new(email)?;

        let user_id = self
            .user_repo
            .create_admin(username, email, password_hash)
            .await?;
        self.namespace_repo
            .create_default(&user_id, "default")
            .await?;
        self.settings_repo.set_bootstrapped().await?;
        Ok(user_id)
    }

    pub async fn is_bootstrapped(&self) -> DomainResult<bool> {
        self.settings_repo.is_bootstrapped().await
    }
}

#[derive(Debug)]
pub struct HealthService;

impl HealthService {
    pub async fn check_liveness() -> DomainResult<()> {
        // Always healthy for now
        Ok(())
    }

    pub async fn check_readiness(&self) -> DomainResult<()> {
        // Placeholder
        Ok(())
    }

    pub async fn self_check(&self) -> DomainResult<()> {
        // Placeholder
        Ok(())
    }
}

impl Clone for HealthService {
    fn clone(&self) -> Self {
        Self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlobPurgeReport {
    pub removed: u64,
    pub freed_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SnapshotPruneReport {
    pub pruned_snapshots: u64,
    pub released_blobs: u64,
}

/// 一次完整维护的结果：先按需裁剪快照，再回收孤儿 blob。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaintenanceRunReport {
    pub pruned_snapshots: u64,
    pub released_blobs: u64,
    pub purged_blobs: u64,
    pub freed_bytes: u64,
}

/// 维护任务：清理没有任何版本或快照引用、且已过保护期的 blob，以及裁剪历史快照。
#[derive(Debug)]
pub struct MaintenanceService<B, E, S> {
    blob_store: B,
    entry_repo: E,
    snapshot_repo: S,
}

impl<B, E, S> MaintenanceService<B, E, S>
where
    B: BlobStore + Send + Sync,
    E: EntryRepo + Send + Sync,
    S: SnapshotRepo + Send + Sync,
{
    pub fn new(blob_store: B, entry_repo: E, snapshot_repo: S) -> Self {
        Self {
            blob_store,
            entry_repo,
            snapshot_repo,
        }
    }

    /// 执行一次完整维护：`snapshot_keep > 0` 时先裁剪快照，随后回收孤儿 blob。
    ///
    /// 裁剪快照会先释放其 blob 引用，因此必须排在回收之前，否则被释放的
    /// blob 要等下一轮才会真正删除。
    pub async fn run_once(
        &self,
        blob_grace_seconds: u64,
        snapshot_keep: u32,
        snapshot_max_age_days: u32,
    ) -> DomainResult<MaintenanceRunReport> {
        let prune = if snapshot_keep > 0 {
            let older_than = (snapshot_max_age_days > 0)
                .then(|| std::time::Duration::from_secs(u64::from(snapshot_max_age_days) * 86_400));
            self.prune_snapshots_with_age(snapshot_keep, older_than)
                .await?
        } else {
            SnapshotPruneReport::default()
        };
        let purge = self.purge_orphan_blobs(blob_grace_seconds).await?;

        Ok(MaintenanceRunReport {
            pruned_snapshots: prune.pruned_snapshots,
            released_blobs: prune.released_blobs,
            purged_blobs: purge.removed,
            freed_bytes: purge.freed_bytes,
        })
    }

    /// 每个命名空间仅保留最新的 `keep` 个快照，删除其余快照并释放其 blob 引用。
    ///
    /// 被删除快照中引用的 blob 会先释放引用计数；归零的 blob 行由
    /// `release_blob_references` 删除，文件随后由调用方（或 `gc-blobs`）清理。
    pub async fn prune_snapshots(&self, keep: u32) -> DomainResult<SnapshotPruneReport> {
        self.prune_snapshots_with_age(keep, None).await
    }

    /// 在数量上限之外，再按**时间窗口**裁剪：只删除「既不在最新 `keep` 个之内、
    /// 又早于 `older_than`（相对当前时间）的快照」。
    ///
    /// `older_than = None` 时退化为纯数量策略；两个条件是「与」的关系，
    /// 因此开启时间窗口不会比只按数量裁剪删得更多。
    pub async fn prune_snapshots_with_age(
        &self,
        keep: u32,
        older_than: Option<std::time::Duration>,
    ) -> DomainResult<SnapshotPruneReport> {
        let mut by_namespace: std::collections::HashMap<NamespaceId, Vec<Snapshot>> =
            std::collections::HashMap::new();
        for snapshot in self.snapshot_repo.list_all_snapshots().await? {
            by_namespace
                .entry(snapshot.namespace_id)
                .or_default()
                .push(snapshot);
        }

        let cutoff = older_than.map(|age| {
            time::OffsetDateTime::now_utc() - time::Duration::seconds(age.as_secs() as i64)
        });
        let mut report = SnapshotPruneReport::default();
        for snapshots in by_namespace.values_mut() {
            // list_all_snapshots 已按创建时间倒序
            for snapshot in snapshots.iter().skip(keep as usize) {
                if let Some(cutoff) = cutoff
                    && snapshot.created_at >= cutoff
                {
                    continue;
                }
                let entries = self
                    .snapshot_repo
                    .get_snapshot_entries(&snapshot.id)
                    .await?;

                let mut counts = std::collections::HashMap::<BlobId, u32>::new();
                for entry in entries {
                    if let Some(blob_id) = entry.blob_id {
                        *counts.entry(blob_id).or_insert(0) += 1;
                    }
                }

                self.snapshot_repo.delete_snapshot(&snapshot.id).await?;
                report.pruned_snapshots += 1;

                let references = counts.into_iter().collect::<Vec<_>>();
                let removable = self.entry_repo.release_blob_references(&references).await?;
                for blob_id in removable {
                    self.blob_store.delete_blob(&blob_id).await?;
                    report.released_blobs += 1;
                }
            }
        }

        Ok(report)
    }

    /// 删除无引用且创建时间早于 `grace_seconds` 之前的 blob，返回清理统计。
    ///
    /// 保护期用于避免误删正在进行中的上传（blob 已落盘但版本尚未建立引用）。
    pub async fn purge_orphan_blobs(&self, grace_seconds: u64) -> DomainResult<BlobPurgeReport> {
        let cutoff = time::OffsetDateTime::now_utc()
            - time::Duration::seconds(i64::try_from(grace_seconds).unwrap_or(i64::MAX));

        let referenced: std::collections::HashSet<BlobId> = self
            .entry_repo
            .referenced_blob_ids()
            .await?
            .into_iter()
            .collect();

        let known_blobs = self.blob_store.list_blobs().await?;
        let known: std::collections::HashSet<BlobId> =
            known_blobs.iter().map(|blob| blob.id).collect();

        let mut report = BlobPurgeReport::default();

        // 1) 有元数据行但无任何引用（异常/崩溃残留）。
        for blob in &known_blobs {
            if !is_purgeable(blob, referenced.contains(&blob.id), cutoff) {
                continue;
            }
            if self.blob_store.purge_blob(&blob.id, blob.ref_count).await? {
                report.removed += 1;
                report.freed_bytes += blob.size_bytes.as_u64();
            }
        }

        // 2) 磁盘上有文件但从未写入元数据行（上传中断留下的孤儿文件）。
        for (blob_id, modified, size) in self.blob_store.list_stored_blob_files().await? {
            if known.contains(&blob_id) || referenced.contains(&blob_id) {
                continue;
            }
            if modified >= cutoff {
                continue;
            }
            self.blob_store.delete_blob(&blob_id).await?;
            report.removed += 1;
            report.freed_bytes += size;
        }

        Ok(report)
    }
}

/// blob 是否可被回收：没有任何引用且早于保护期截止时间。
pub fn is_purgeable(blob: &Blob, is_referenced: bool, cutoff: time::OffsetDateTime) -> bool {
    !is_referenced && blob.created_at < cutoff
}

#[derive(Debug, Clone)]
pub struct SessionBootstrap {
    pub auth_enabled: bool,
    pub current_user: Option<UserId>,
    pub capabilities: Vec<Capability>,
    pub active_workspace: Option<NamespaceId>,
    pub features: FeatureMatrix,
}

#[derive(Debug)]
pub struct SessionService {
    auth_service: Option<AuthService>,
    features: FeatureMatrix,
}

impl SessionService {
    pub fn new(auth_service: Option<AuthService>, features: FeatureMatrix) -> Self {
        Self {
            auth_service,
            features,
        }
    }

    pub async fn bootstrap(&self, auth_token: Option<&str>) -> DomainResult<SessionBootstrap> {
        let (auth_enabled, current_user, capabilities) =
            if let Some(auth_service) = &self.auth_service {
                if let Some(token) = auth_token {
                    match auth_service.authenticate_session(token).await {
                        Ok(auth_user) => {
                            let capabilities = self
                                .features
                                .capabilities_for_role(auth_user.role.can_access_admin_panel());
                            (true, Some(auth_user.id), capabilities)
                        }
                        Err(_) => {
                            // Invalid token, treat as unauthenticated
                            (true, None, vec![])
                        }
                    }
                } else {
                    // Auth enabled but no token provided
                    (true, None, vec![])
                }
            } else {
                // Auth disabled
                (
                    false,
                    None,
                    vec![Capability::Upload, Capability::ViewHistory],
                )
            };

        Ok(SessionBootstrap {
            auth_enabled,
            current_user,
            capabilities,
            active_workspace: None, // TODO: implement workspaces
            features: self.features.clone(),
        })
    }
}

impl Clone for SessionService {
    fn clone(&self) -> Self {
        Self {
            auth_service: self.auth_service.clone(),
            features: self.features.clone(),
        }
    }
}

#[derive(Debug)]
pub struct AuthService {
    user_repo: SqliteUserRepo,
    session_repo: SqliteSessionRepo,
    session_ttl_seconds: u64,
    /// r9 热验缓存 ✗ key = SHA-256(cred)（原文零落盘 ✗）只存成功 + User clone +
    /// TTL 30s（改密码 30s 窗记档 ✗ 禁用 = 命中时查 user.disabled 即时生效 ✓
    /// 失败路径不缓存 = 爆破/timing 防护零损 ✓ Arc<Mutex> = Clone 后共享不分叉 ✓）。
    verified_cache: VerifiedCredentialCache,
}

type VerifiedCredentialCache = std::sync::Arc<
    std::sync::Mutex<std::collections::HashMap<[u8; 32], (User, std::time::Instant)>>,
>;

impl AuthService {
    pub fn new(
        user_repo: SqliteUserRepo,
        session_repo: SqliteSessionRepo,
        session_ttl_seconds: u64,
    ) -> Self {
        Self {
            user_repo,
            session_repo,
            session_ttl_seconds,
            verified_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    pub async fn register(&self, req: RegisterRequest) -> DomainResult<User> {
        let username = Username::new(&req.username)?;
        let email = normalize_message(req.email.as_deref())
            .map(|value| EmailAddress::new(&value))
            .transpose()?;

        // Check if user already exists
        if self.user_repo.find_by_username(&username).await.is_ok() {
            return Err(DomainError::Conflict {
                message: "Username already exists".to_string(),
            });
        }
        if let Some(email) = email.as_ref()
            && self.user_repo.find_by_email(email).await.is_ok()
        {
            return Err(DomainError::Conflict {
                message: "Email already exists".to_string(),
            });
        }

        // Hash password (placeholder - in real impl, use proper hashing)
        let password_hash = self.hash_password(&req.password)?;

        let user_id = self
            .user_repo
            .create_user(&username, email.as_ref(), &password_hash, Role::User)
            .await?;
        self.user_repo.find_by_id(&user_id).await
    }

    pub async fn login(&self, req: LoginRequest) -> DomainResult<LoginResponse> {
        // 标识/口令校验（含旧哈希透明升级）与 FTP 认证共用同一实现
        let user = self
            .verify_credentials(&req.username_or_email, &req.password)
            .await?;

        // Create session
        let now = time::OffsetDateTime::now_utc();
        let expires_at = now + time::Duration::seconds(self.session_ttl_seconds as i64);
        let token = uuid::Uuid::new_v4().to_string();
        let token_hash = self.hash_token(&token)?;

        let session_id = self
            .session_repo
            .create_session(
                &user.id,
                &token_hash,
                expires_at,
                None, // user_agent
                None, // ip_addr
            )
            .await?;

        let session = UserSession {
            id: session_id,
            user_id: user.id,
            session_token_hash: token_hash,
            issued_at: now,
            expires_at,
            revoked_at: None,
            user_agent: None,
            ip_addr: None,
            last_seen_at: now,
        };

        Ok(LoginResponse {
            user,
            session,
            token,
        })
    }

    /// 仅校验用户名/口令并返回用户，不创建会话。
    ///
    /// HTTP 登录与 FTP 认证共用这段逻辑（含禁用校验、旧 SHA-256 哈希透明升级），
    /// 避免两处实现出现安全差异。
    /// 凭据键（r9 ✗ 纯函数单测 ✓ SHA-256(username ∥ 0 ∥ password) 原文零存）。
    fn cred_key(username_or_email: &str, password: &str) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(username_or_email.as_bytes());
        hasher.update([0u8]);
        hasher.update(password.as_bytes());
        hasher.finalize().into()
    }

    pub async fn verify_credentials(
        &self,
        username_or_email: &str,
        password: &str,
    ) -> DomainResult<User> {
        // r9 热验缓存命中（TTL 30s ✗ disabled 每命中实时查 = 禁用即时生效 ✓）
        {
            let key = Self::cred_key(username_or_email, password);
            let cache = self.verified_cache.lock().expect("auth cache poisoned");
            if let Some((user, at)) = cache.get(&key)
                && at.elapsed().as_secs() < 30
                && !user.disabled
            {
                return Ok(user.clone());
            }
        }
        // 无效标识与口令错误返回同一个错误，避免泄露「用户名是否存在」
        let user = if username_or_email.contains('@') {
            let email = EmailAddress::new(username_or_email)
                .map_err(|_| DomainError::InvalidCredentials)?;
            match self.user_repo.find_by_email(&email).await {
                Ok(user) => user,
                Err(DomainError::NotFound { .. }) => return Err(DomainError::InvalidCredentials),
                Err(err) => return Err(err),
            }
        } else {
            let username =
                Username::new(username_or_email).map_err(|_| DomainError::InvalidCredentials)?;
            match self.user_repo.find_by_username(&username).await {
                Ok(user) => user,
                Err(DomainError::NotFound { .. }) => return Err(DomainError::InvalidCredentials),
                Err(err) => return Err(err),
            }
        };

        if user.disabled {
            return Err(DomainError::InvalidCredentials);
        }

        if !self.verify_password(password, &user.password_hash)? {
            return Err(DomainError::InvalidCredentials);
        }
        // r9 成功才入缓存 ✗ 顺手驱逐过期（O(n) 小 n ✓ 失败零缓存 = 爆破防护 ✓）
        {
            let key = Self::cred_key(username_or_email, password);
            let mut cache = self.verified_cache.lock().expect("auth cache poisoned");
            cache.retain(|_, (_, at)| at.elapsed().as_secs() < 30);
            cache.insert(key, (user.clone(), std::time::Instant::now()));
        }

        // 旧 SHA-256 哈希在成功校验后透明升级；失败不得影响本次登录
        if !user.password_hash.starts_with("$argon2")
            && let Ok(new_hash) = self.hash_password(password)
        {
            let _ = self.user_repo.update_password(&user.id, &new_hash).await;
        }

        Ok(user)
    }

    pub async fn authenticate_session(&self, token: &str) -> DomainResult<AuthUser> {
        let token_hash = self.hash_token(token)?;
        let session = self
            .session_repo
            .find_session_by_token_hash(&token_hash)
            .await?;

        if session.revoked_at.is_some() {
            return Err(DomainError::SessionRevoked);
        }

        let now = time::OffsetDateTime::now_utc();
        if session.expires_at < now {
            return Err(DomainError::SessionExpired);
        }

        // Update last seen
        self.session_repo.update_last_seen(&session.id).await?;

        let user = self.user_repo.find_by_id(&session.user_id).await?;

        if user.disabled {
            return Err(DomainError::UserDisabled);
        }

        Ok(AuthUser {
            id: user.id,
            username: user.username,
            email: user.email,
            role: user.role,
        })
    }

    pub async fn logout(&self, token: &str) -> DomainResult<()> {
        let token_hash = self.hash_token(token)?;
        let session = self
            .session_repo
            .find_session_by_token_hash(&token_hash)
            .await?;
        self.session_repo.revoke_session(&session.id).await
    }

    pub fn user_repo(&self) -> &SqliteUserRepo {
        &self.user_repo
    }

    pub fn session_repo(&self) -> &SqliteSessionRepo {
        &self.session_repo
    }

    pub fn hash_password_for_storage(password: &str) -> DomainResult<String> {
        use argon2::{Argon2, PasswordHasher, password_hash::SaltString};

        let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).map_err(|err| {
            DomainError::Internal {
                message: format!("Failed to generate password salt: {}", err),
            }
        })?;
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|err| DomainError::Internal {
                message: format!("Failed to hash password: {}", err),
            })
    }

    pub fn verify_password_for_storage(password: &str, hash: &str) -> bool {
        if hash.starts_with("$argon2") {
            use argon2::{Argon2, PasswordHash, PasswordVerifier};
            let Ok(parsed_hash) = PasswordHash::new(hash) else {
                return false;
            };
            return Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok();
        }

        // Legacy SHA-256 hashes are accepted for existing installations and
        // upgraded transparently on the next successful login.
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest;
        hasher.update(password.as_bytes());
        hex::encode(hasher.finalize()) == hash
    }

    fn hash_password(&self, password: &str) -> DomainResult<String> {
        Self::hash_password_for_storage(password)
    }

    fn verify_password(&self, password: &str, hash: &str) -> DomainResult<bool> {
        Ok(Self::verify_password_for_storage(password, hash))
    }

    fn hash_token(&self, token: &str) -> DomainResult<String> {
        // Use SHA256 for token hashing
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        Ok(hex::encode(hasher.finalize()))
    }
}

impl Clone for AuthService {
    fn clone(&self) -> Self {
        // Note: Cloning repos is not ideal, but for HTTP layer we need Clone
        // In a real implementation, consider using Arc or dependency injection
        Self {
            user_repo: self.user_repo.clone(),
            session_repo: self.session_repo.clone(),
            session_ttl_seconds: self.session_ttl_seconds,
            // r9 Clone = Arc 共享缓存（新建空 = 分叉 ✗ 共享才免疫 clone 分叉 ✓）
            verified_cache: self.verified_cache.clone(),
        }
    }
}

// File system services
#[derive(Debug, Clone)]
pub struct TreeItem {
    pub entry_id: EntryId,
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    pub size_bytes: Option<u64>,
    pub created_at: time::OffsetDateTime,
    pub modified_at: Option<time::OffsetDateTime>,
    pub version_id: Option<VersionId>,
    pub mime_type: Option<String>,
    pub is_text: Option<bool>,
    pub preview_kind: Option<String>,
    pub last_change: Option<ChangeSummary>,
}

#[derive(Debug, Clone)]
pub struct ChangeSummary {
    pub actor_name: String,
    pub message: Option<String>,
    pub at: time::OffsetDateTime,
    pub snapshot_id: Option<SnapshotId>,
    pub version_id: Option<VersionId>,
}

#[derive(Debug, Clone)]
pub struct TreeResponse {
    pub path: String,
    pub mode: TreeMode,
    pub active_snapshot: Option<SnapshotId>,
    pub breadcrumbs: Vec<BreadcrumbItem>,
    pub items: Vec<TreeItem>,
    pub permissions: Vec<Capability>,
    pub summary: TreeSummary,
}

#[derive(Debug, Clone)]
pub enum TreeMode {
    Live,
    Snapshot,
}

#[derive(Debug, Clone)]
pub struct BreadcrumbItem {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct TreeSummary {
    pub total_files: u32,
    pub total_dirs: u32,
    pub total_size_bytes: u64,
}

enum RequestedFileRevision {
    Live,
    Version(VersionId),
    Snapshot(SnapshotId),
}

#[derive(Debug, Clone)]
struct ResolvedFileBlob {
    filename: String,
    mime_type: Option<String>,
    size_bytes: u64,
    blob_id: BlobId,
    modified_at: Option<time::OffsetDateTime>,
}

pub struct FileContentStream {
    pub filename: String,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub etag: String,
    pub modified_at: Option<time::OffsetDateTime>,
    pub reader: Box<dyn vfiles_domain::ReadSeek + Send + Unpin>,
}

#[derive(Debug, Clone)]
pub struct FileContentBytes {
    pub blob_id: BlobId,
    pub filename: String,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub bytes: Vec<u8>,
}

pub struct DirectoryArchive {
    pub filename: String,
    pub size_bytes: u64,
    pub reader: Box<dyn ReadSeek + Send + Unpin>,
}

enum ArchiveWriterMessage {
    StartFile(String),
    Data(Vec<u8>),
    Finish,
}

#[derive(Debug, Clone)]
pub struct WorkspaceService<E, S, B, U> {
    entry_repo: E,
    snapshot_repo: S,
    blob_store: B,
    _upload_store: U,
}

impl<E, S, B, U> WorkspaceService<E, S, B, U>
where
    E: EntryRepo + Send + Sync,
    S: SnapshotRepo + Send + Sync,
    B: BlobStore + Send + Sync,
    U: UploadStore,
{
    pub fn new(entry_repo: E, snapshot_repo: S, blob_store: B, upload_store: U) -> Self {
        Self {
            entry_repo,
            snapshot_repo,
            blob_store,
            _upload_store: upload_store,
        }
    }

    async fn resolve_requested_file_revision(
        &self,
        raw_commit: Option<&str>,
    ) -> DomainResult<RequestedFileRevision> {
        let Some(raw_commit) = raw_commit.filter(|value| !value.trim().is_empty()) else {
            return Ok(RequestedFileRevision::Live);
        };

        match SnapshotId::from_string(raw_commit) {
            Ok(snapshot_id) => match self.snapshot_repo.find_snapshot(&snapshot_id).await {
                Ok(_) => Ok(RequestedFileRevision::Snapshot(snapshot_id)),
                Err(DomainError::SnapshotNotFound) => Ok(RequestedFileRevision::Version(
                    VersionId::from_string(raw_commit).map_err(|_| invalid_commit_error())?,
                )),
                Err(err) => Err(err),
            },
            Err(_) => Ok(RequestedFileRevision::Version(
                VersionId::from_string(raw_commit).map_err(|_| invalid_commit_error())?,
            )),
        }
    }

    async fn resolve_file_version_for_path(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        requested_version: Option<&VersionId>,
    ) -> DomainResult<Option<EntryVersion>> {
        let Some(entry) = self.entry_repo.find_by_path(namespace_id, path).await? else {
            return Ok(None);
        };
        if entry.entry_type != EntryKind::File {
            return Ok(None);
        }

        if let Some(version_id) = requested_version {
            let version = self.entry_repo.find_version(version_id).await?;
            if version.entry_id != entry.id {
                return Ok(None);
            }
            return Ok(Some(version));
        }

        let Some(current_version_id) = entry.current_version_id else {
            return Ok(None);
        };

        Ok(Some(
            self.entry_repo.find_version(&current_version_id).await?,
        ))
    }

    async fn resolve_snapshot_file_entry(
        &self,
        path: &NormalizedPath,
        snapshot_id: &SnapshotId,
    ) -> DomainResult<Option<SnapshotEntry>> {
        let entries = self.snapshot_repo.get_snapshot_entries(snapshot_id).await?;
        Ok(entries.into_iter().find(|entry| {
            entry.entry_path == *path
                && entry.entry_kind == EntryKind::File
                && entry.change_type != ChangeType::Deleted
        }))
    }

    async fn collect_scoped_entries(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> DomainResult<Vec<Entry>> {
        if path.as_str().is_empty() {
            return collect_namespace_entries(&self.entry_repo, namespace_id).await;
        }

        if let Some(entry) = self.entry_repo.find_by_path(namespace_id, path).await? {
            if entry.entry_type != EntryKind::Directory {
                return Err(DomainError::Validation {
                    message: "Path is not a directory".to_string(),
                });
            }
            return collect_descendants(&self.entry_repo, namespace_id, &entry).await;
        }

        let entries = collect_namespace_entries(&self.entry_repo, namespace_id).await?;
        let scoped_entries = entries
            .into_iter()
            .filter(|entry| path_matches_scope(&entry.path_norm, path))
            .collect::<Vec<_>>();
        if scoped_entries.is_empty() {
            return Err(DomainError::NotFound {
                resource: "entry".to_string(),
            });
        }

        Ok(scoped_entries)
    }

    async fn validate_directory_scope(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> DomainResult<()> {
        if path.as_str().is_empty() {
            return Ok(());
        }
        if let Some(entry) = self.entry_repo.find_by_path(namespace_id, path).await? {
            return if entry.entry_type == EntryKind::Directory {
                Ok(())
            } else {
                Err(DomainError::Validation {
                    message: "Path is not a directory".to_string(),
                })
            };
        }

        let entries = collect_namespace_entries(&self.entry_repo, namespace_id).await?;
        if entries
            .iter()
            .any(|entry| path_matches_scope(&entry.path_norm, path))
        {
            Ok(())
        } else {
            Err(DomainError::NotFound {
                resource: "entry".to_string(),
            })
        }
    }

    async fn collect_live_directory_files(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> DomainResult<Vec<(NormalizedPath, BlobId)>> {
        let scope_entries = self.collect_scoped_entries(namespace_id, path).await?;

        let version_ids = scope_entries
            .iter()
            .filter(|entry| entry.entry_type == EntryKind::File)
            .filter_map(|entry| entry.current_version_id)
            .collect::<Vec<_>>();
        let versions_by_id = self
            .entry_repo
            .find_versions(&version_ids)
            .await?
            .into_iter()
            .map(|version| (version.id, version))
            .collect::<HashMap<_, _>>();

        let mut files = Vec::new();
        for entry in scope_entries {
            if entry.entry_type != EntryKind::File {
                continue;
            }
            let Some(version_id) = entry.current_version_id else {
                continue;
            };
            let Some(version) = versions_by_id.get(&version_id) else {
                // Preserve the repository's missing-version error if metadata is inconsistent.
                self.entry_repo.find_version(&version_id).await?;
                continue;
            };
            let Some(blob_id) = version.blob_id else {
                continue;
            };
            files.push((entry.path_norm.clone(), blob_id));
        }

        files.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));
        Ok(files)
    }

    async fn collect_versioned_directory_files(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        version_id: &VersionId,
    ) -> DomainResult<Vec<(NormalizedPath, BlobId)>> {
        let cutoff = self.entry_repo.find_version(version_id).await?.created_at;
        let scope_entries = self.collect_scoped_entries(namespace_id, path).await?;
        let file_entries = scope_entries
            .iter()
            .filter(|entry| entry.entry_type == EntryKind::File)
            .collect::<Vec<_>>();
        let entry_ids = file_entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let versions_by_entry = self
            .entry_repo
            .find_versions_for_entries_before(&entry_ids, cutoff)
            .await?
            .into_iter()
            .map(|version| (version.entry_id, version))
            .collect::<HashMap<_, _>>();

        let mut files = Vec::new();
        for entry in file_entries {
            let Some(version) = versions_by_entry.get(&entry.id) else {
                continue;
            };
            let Some(blob_id) = version.blob_id else {
                continue;
            };
            files.push((entry.path_norm.clone(), blob_id));
        }

        files.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));
        Ok(files)
    }

    async fn collect_snapshot_directory_files(
        &self,
        path: &NormalizedPath,
        snapshot_id: &SnapshotId,
    ) -> DomainResult<Vec<(NormalizedPath, BlobId)>> {
        let snapshot_entries = self.snapshot_repo.get_snapshot_entries(snapshot_id).await?;
        validate_snapshot_directory_scope(path, &snapshot_entries)?;
        let visible_entries = snapshot_entries
            .iter()
            .filter(|entry| entry.change_type != ChangeType::Deleted)
            .collect::<Vec<_>>();

        let mut files = visible_entries
            .into_iter()
            .filter(|entry| {
                entry.entry_kind == EntryKind::File && path_matches_scope(&entry.entry_path, path)
            })
            .filter_map(|entry| {
                entry
                    .blob_id
                    .map(|blob_id| (entry.entry_path.clone(), blob_id))
            })
            .collect::<Vec<_>>();
        files.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));
        Ok(files)
    }

    async fn build_directory_archive(
        &self,
        archive_name: &str,
        requested_path: &NormalizedPath,
        files: Vec<(NormalizedPath, BlobId)>,
    ) -> DomainResult<(Box<dyn ReadSeek + Send + Unpin>, u64)> {
        // Keep both archive bytes and deflate work off async worker threads. The bounded channel
        // applies backpressure so a fast blob reader cannot queue an entire archive in memory.
        let (writer_tx, mut writer_rx) = tokio::sync::mpsc::channel(2);
        let writer = tokio::task::spawn_blocking(move || -> Result<std::fs::File, String> {
            let output = tempfile::tempfile()
                .map_err(|e| format!("Failed to create temporary archive: {e}"))?;
            let mut zip = zip::ZipWriter::new(output);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            loop {
                match writer_rx.blocking_recv() {
                    Some(ArchiveWriterMessage::StartFile(path)) => {
                        zip.start_file(path, options)
                            .map_err(|e| format!("Failed to create zip entry: {e}"))?;
                    }
                    Some(ArchiveWriterMessage::Data(bytes)) => zip
                        .write_all(&bytes)
                        .map_err(|e| format!("Failed to write zip entry: {e}"))?,
                    Some(ArchiveWriterMessage::Finish) => break,
                    None => return Err("Archive input stream closed before finishing".to_string()),
                }
            }

            let mut output = zip
                .finish()
                .map_err(|e| format!("Failed to finalize zip archive: {e}"))?;
            output
                .flush()
                .map_err(|e| format!("Failed to flush zip archive: {e}"))?;
            Ok(output)
        });

        for (full_path, blob_id) in files {
            let mut reader = self
                .blob_store
                .get_blob_stream(&blob_id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    resource: "blob data".to_string(),
                })?;
            let mut buffer = [0_u8; 64 * 1024];
            let relative = relative_path_for_directory(requested_path, &full_path);
            let zip_path = if relative.is_empty() {
                archive_name.to_string()
            } else {
                format!("{}/{}", archive_name, relative)
            };

            writer_tx
                .send(ArchiveWriterMessage::StartFile(zip_path))
                .await
                .map_err(|_| DomainError::Internal {
                    message: "Archive writer stopped unexpectedly".to_string(),
                })?;

            loop {
                let read = reader
                    .read(&mut buffer)
                    .await
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to stream blob data: {}", e),
                    })?;
                if read == 0 {
                    break;
                }

                writer_tx
                    .send(ArchiveWriterMessage::Data(buffer[..read].to_vec()))
                    .await
                    .map_err(|_| DomainError::Internal {
                        message: "Archive writer stopped unexpectedly".to_string(),
                    })?;
            }
        }

        writer_tx
            .send(ArchiveWriterMessage::Finish)
            .await
            .map_err(|_| DomainError::Internal {
                message: "Archive writer stopped unexpectedly".to_string(),
            })?;
        drop(writer_tx);
        let mut output = writer
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Archive writer task failed: {e}"),
            })?
            .map_err(|e| DomainError::Internal { message: e })?;
        let size_bytes = output
            .metadata()
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to stat temporary archive: {e}"),
            })?
            .len();
        std::io::Seek::seek(&mut output, std::io::SeekFrom::Start(0)).map_err(|e| {
            DomainError::Internal {
                message: format!("Failed to rewind temporary archive: {e}"),
            }
        })?;
        Ok((Box::new(tokio::fs::File::from_std(output)), size_bytes))
    }

    async fn resolve_file_blob(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        raw_commit: Option<&str>,
    ) -> DomainResult<ResolvedFileBlob> {
        let filename = basename(path).to_string();

        match self.resolve_requested_file_revision(raw_commit).await? {
            RequestedFileRevision::Live => {
                let version = self
                    .resolve_file_version_for_path(namespace_id, path, None)
                    .await?
                    .ok_or_else(|| DomainError::NotFound {
                        resource: "file version".to_string(),
                    })?;
                let blob_id = version.blob_id.ok_or_else(|| DomainError::NotFound {
                    resource: "blob".to_string(),
                })?;

                Ok(ResolvedFileBlob {
                    filename,
                    mime_type: version.mime_type.clone(),
                    size_bytes: version.size_bytes.as_u64(),
                    blob_id,
                    modified_at: Some(version.created_at),
                })
            }
            RequestedFileRevision::Version(version_id) => {
                let version = self
                    .resolve_file_version_for_path(namespace_id, path, Some(&version_id))
                    .await?
                    .ok_or_else(|| DomainError::NotFound {
                        resource: "file version".to_string(),
                    })?;
                let blob_id = version.blob_id.ok_or_else(|| DomainError::NotFound {
                    resource: "blob".to_string(),
                })?;

                Ok(ResolvedFileBlob {
                    filename,
                    mime_type: version.mime_type.clone(),
                    size_bytes: version.size_bytes.as_u64(),
                    blob_id,
                    modified_at: Some(version.created_at),
                })
            }
            RequestedFileRevision::Snapshot(snapshot_id) => {
                let entry = self
                    .resolve_snapshot_file_entry(path, &snapshot_id)
                    .await?
                    .ok_or_else(|| DomainError::NotFound {
                        resource: "snapshot file".to_string(),
                    })?;
                let blob_id = entry.blob_id.ok_or_else(|| DomainError::NotFound {
                    resource: "blob".to_string(),
                })?;

                Ok(ResolvedFileBlob {
                    filename,
                    mime_type: entry.mime_type.clone(),
                    size_bytes: entry.size_bytes.map(|value| value.as_u64()).unwrap_or(0),
                    blob_id,
                    modified_at: entry.created_at,
                })
            }
        }
    }

    pub async fn open_file(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        raw_commit: Option<&str>,
    ) -> DomainResult<FileContentStream> {
        let resolved = self
            .resolve_file_blob(namespace_id, path, raw_commit)
            .await?;
        let reader = self
            .blob_store
            .get_blob_stream(&resolved.blob_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "blob data".to_string(),
            })?;

        Ok(FileContentStream {
            filename: resolved.filename,
            mime_type: resolved.mime_type,
            size_bytes: resolved.size_bytes,
            etag: format!("\"{}\"", resolved.blob_id),
            modified_at: resolved.modified_at,
            reader,
        })
    }

    pub async fn read_file_bytes(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        raw_commit: Option<&str>,
    ) -> DomainResult<FileContentBytes> {
        let resolved = self
            .resolve_file_blob(namespace_id, path, raw_commit)
            .await?;
        let bytes = self
            .blob_store
            .get_blob(&resolved.blob_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "blob data".to_string(),
            })?;

        Ok(FileContentBytes {
            blob_id: resolved.blob_id,
            filename: resolved.filename,
            mime_type: resolved.mime_type,
            size_bytes: resolved.size_bytes,
            bytes,
        })
    }

    pub async fn download_directory_archive(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        raw_commit: Option<&str>,
    ) -> DomainResult<DirectoryArchive> {
        let archive_name = archive_name_for(path);
        let files = match self.resolve_requested_file_revision(raw_commit).await? {
            RequestedFileRevision::Live => {
                self.collect_live_directory_files(namespace_id, path)
                    .await?
            }
            RequestedFileRevision::Version(version_id) => {
                self.collect_versioned_directory_files(namespace_id, path, &version_id)
                    .await?
            }
            RequestedFileRevision::Snapshot(snapshot_id) => {
                self.collect_snapshot_directory_files(path, &snapshot_id)
                    .await?
            }
        };
        let (reader, size_bytes) = self
            .build_directory_archive(&archive_name, path, files)
            .await?;

        Ok(DirectoryArchive {
            filename: format!("{}.zip", archive_name),
            size_bytes,
            reader,
        })
    }

    /// Validate the selected directory archive target without reading blobs or building a ZIP.
    /// Conditional requests use this to preserve path/revision errors while avoiding compression.
    pub async fn validate_directory_archive_target(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        raw_commit: Option<&str>,
    ) -> DomainResult<()> {
        match self.resolve_requested_file_revision(raw_commit).await? {
            RequestedFileRevision::Live => {
                self.validate_directory_scope(namespace_id, path).await?;
            }
            RequestedFileRevision::Version(version_id) => {
                self.entry_repo.find_version(&version_id).await?;
                self.validate_directory_scope(namespace_id, path).await?;
            }
            RequestedFileRevision::Snapshot(snapshot_id) => {
                let snapshot_entries = self
                    .snapshot_repo
                    .get_snapshot_entries(&snapshot_id)
                    .await?;
                validate_snapshot_directory_scope(path, &snapshot_entries)?;
            }
        }
        Ok(())
    }

    async fn live_tree(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> DomainResult<TreeResponse> {
        if !path.as_str().is_empty() {
            let entry = self
                .entry_repo
                .find_by_path(namespace_id, path)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    resource: "entry".to_string(),
                })?;

            if entry.entry_type != EntryKind::Directory {
                return Err(DomainError::Validation {
                    message: "Path is not a directory".to_string(),
                });
            }
        }

        let children = self.entry_repo.find_children(namespace_id, path).await?;

        let items = self.build_tree_items(children).await?;

        let summary = TreeSummary {
            total_files: items
                .iter()
                .filter(|item| item.kind == EntryKind::File)
                .count() as u32,
            total_dirs: items
                .iter()
                .filter(|item| item.kind == EntryKind::Directory)
                .count() as u32,
            total_size_bytes: items.iter().filter_map(|item| item.size_bytes).sum(),
        };

        Ok(TreeResponse {
            path: path.as_str().to_string(),
            mode: TreeMode::Live,
            active_snapshot: None,
            breadcrumbs: breadcrumbs_for(path),
            items,
            permissions: vec![Capability::Upload, Capability::ViewHistory],
            summary,
        })
    }

    /// 按页取子目录内容：SQL 侧分页 + 总数，避免大目录下每次请求都拉全量。
    ///
    /// 排序与 `live_tree` 一致（目录优先 + 名称升序），因此分页结果与原来的
    /// 「全量取回后切片」完全一致。
    pub async fn live_children_page(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        limit: u32,
        offset: u32,
    ) -> DomainResult<(Vec<TreeItem>, u64)> {
        if !path.as_str().is_empty() {
            let entry = self
                .entry_repo
                .find_by_path(namespace_id, path)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    resource: "entry".to_string(),
                })?;

            if entry.entry_type != EntryKind::Directory {
                return Err(DomainError::Validation {
                    message: "Path is not a directory".to_string(),
                });
            }
        }

        let (children, total) = self
            .entry_repo
            .find_children_page(namespace_id, path, limit, offset)
            .await?;
        let items = self.build_tree_items(children).await?;
        Ok((items, total))
    }

    /// 批量补全版本信息并排序，返回可展示的条目列表。
    async fn build_tree_items(&self, children: Vec<Entry>) -> DomainResult<Vec<TreeItem>> {
        // 批量取当前版本，避免逐个 child 查询造成 N+1。
        let version_ids: Vec<VersionId> = children
            .iter()
            .filter_map(|child| match (child.entry_type, child.current_version_id) {
                (EntryKind::File, Some(version_id)) => Some(version_id),
                _ => None,
            })
            .collect();
        let versions_by_id: HashMap<VersionId, EntryVersion> = self
            .entry_repo
            .find_versions(&version_ids)
            .await?
            .into_iter()
            .map(|version| (version.id, version))
            .collect();

        let mut items = Vec::with_capacity(children.len());

        for child in children {
            let version = match (child.entry_type, child.current_version_id) {
                (EntryKind::File, Some(version_id)) => versions_by_id.get(&version_id),
                _ => None,
            };
            items.push(TreeItem {
                entry_id: child.id,
                name: child.name.clone(),
                path: child.path_norm.as_str().to_string(),
                kind: child.entry_type,
                size_bytes: version.as_ref().map(|value| value.size_bytes.as_u64()),
                created_at: child.created_at,
                modified_at: version.as_ref().map(|value| value.created_at),
                version_id: version.as_ref().map(|value| value.id),
                mime_type: version.as_ref().and_then(|value| value.mime_type.clone()),
                is_text: version.as_ref().map(|value| value.is_text),
                preview_kind: preview_kind(
                    version
                        .as_ref()
                        .and_then(|value| value.mime_type.as_deref()),
                ),
                last_change: version.as_ref().map(|value| ChangeSummary {
                    actor_name: actor_name_for(value),
                    message: message_for(value),
                    at: value.created_at,
                    snapshot_id: None,
                    version_id: Some(value.id),
                }),
            });
        }

        items.sort_by(|left, right| {
            let kind_order = match (left.kind, right.kind) {
                (EntryKind::Directory, EntryKind::File) => std::cmp::Ordering::Less,
                (EntryKind::File, EntryKind::Directory) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            };

            kind_order.then_with(|| left.name.cmp(&right.name))
        });

        Ok(items)
    }

    async fn snapshot_tree(
        &self,
        path: &NormalizedPath,
        snapshot_id: &SnapshotId,
    ) -> DomainResult<TreeResponse> {
        let snapshot = self.snapshot_repo.find_snapshot(snapshot_id).await?;
        let snapshot_entries = self.snapshot_repo.get_snapshot_entries(snapshot_id).await?;
        let visible_entries = snapshot_entries
            .iter()
            .filter(|entry| entry.change_type != ChangeType::Deleted)
            .collect::<Vec<_>>();

        if !path.as_str().is_empty() {
            if let Some(entry) = visible_entries
                .iter()
                .find(|entry| entry.entry_path == *path)
            {
                if entry.entry_kind != EntryKind::Directory {
                    return Err(DomainError::Validation {
                        message: "Path is not a directory".to_string(),
                    });
                }
            } else {
                let prefix = format!("{}/", path.as_str());
                let has_descendants = visible_entries
                    .iter()
                    .any(|entry| entry.entry_path.as_str().starts_with(&prefix));
                if !has_descendants {
                    return Err(DomainError::NotFound {
                        resource: "snapshot entry".to_string(),
                    });
                }
            }
        }

        let prefix = if path.as_str().is_empty() {
            None
        } else {
            Some(format!("{}/", path.as_str()))
        };
        let mut items = std::collections::BTreeMap::new();

        for snapshot_entry in &visible_entries {
            let remainder = if path.as_str().is_empty() {
                snapshot_entry.entry_path.as_str()
            } else if snapshot_entry.entry_path == *path {
                continue;
            } else if let Some(value) = prefix
                .as_ref()
                .and_then(|value| snapshot_entry.entry_path.as_str().strip_prefix(value))
            {
                value
            } else {
                continue;
            };

            if remainder.is_empty() {
                continue;
            }

            if let Some((segment, _)) = remainder.split_once('/') {
                let child_path = join_path(path, segment)?;
                let child_key = child_path.as_str().to_string();
                items.entry(child_key.clone()).or_insert_with(|| TreeItem {
                    entry_id: synthetic_snapshot_entry_id(snapshot_id, &child_path),
                    name: segment.to_string(),
                    path: child_key,
                    kind: EntryKind::Directory,
                    size_bytes: None,
                    created_at: snapshot.created_at,
                    modified_at: Some(snapshot.created_at),
                    version_id: None,
                    mime_type: None,
                    is_text: None,
                    preview_kind: None,
                    last_change: None,
                });
            } else {
                items.insert(
                    snapshot_entry.entry_path.as_str().to_string(),
                    tree_item_from_snapshot(&snapshot, snapshot_entry),
                );
            }
        }

        let mut items = items.into_values().collect::<Vec<_>>();
        items.sort_by(|left, right| {
            let kind_order = match (left.kind, right.kind) {
                (EntryKind::Directory, EntryKind::File) => std::cmp::Ordering::Less,
                (EntryKind::File, EntryKind::Directory) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            };

            kind_order.then_with(|| left.name.cmp(&right.name))
        });

        let summary = TreeSummary {
            total_files: items
                .iter()
                .filter(|item| item.kind == EntryKind::File)
                .count() as u32,
            total_dirs: items
                .iter()
                .filter(|item| item.kind == EntryKind::Directory)
                .count() as u32,
            total_size_bytes: items.iter().filter_map(|item| item.size_bytes).sum(),
        };

        Ok(TreeResponse {
            path: path.as_str().to_string(),
            mode: TreeMode::Snapshot,
            active_snapshot: Some(snapshot.id),
            breadcrumbs: breadcrumbs_for(path),
            items,
            permissions: vec![Capability::Upload, Capability::ViewHistory],
            summary,
        })
    }

    pub async fn create_snapshot(
        &self,
        namespace_id: &NamespaceId,
        message: Option<&str>,
        user_id: &UserId,
    ) -> DomainResult<Snapshot> {
        let snapshot_entries =
            collect_snapshot_state(&self.entry_repo, namespace_id, Vec::new()).await?;
        let snapshot_id = create_snapshot_record(
            &self.snapshot_repo,
            namespace_id,
            message,
            SnapshotKind::UserCreated,
            user_id,
            snapshot_entries,
        )
        .await?;

        self.snapshot_repo.find_snapshot(&snapshot_id).await
    }

    pub async fn tree(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        snapshot_id: Option<&SnapshotId>,
    ) -> DomainResult<TreeResponse> {
        if let Some(snapshot_id) = snapshot_id {
            self.snapshot_tree(path, snapshot_id).await
        } else {
            self.live_tree(namespace_id, path).await
        }
    }

    pub async fn create_directory(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
    ) -> DomainResult<MutationResult> {
        if path.as_str().is_empty() {
            return Err(DomainError::Validation {
                message: "Cannot create the root directory".to_string(),
            });
        }

        let parent_path = path
            .as_str()
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        let parent_path =
            NormalizedPath::new(parent_path).map_err(|_| DomainError::Validation {
                message: "Invalid parent path".to_string(),
            })?;

        let mut changed_entries =
            ensure_directory_path(&self.entry_repo, namespace_id, &parent_path, user_id).await?;
        let entry_id = self
            .entry_repo
            .create_entry(namespace_id, path, EntryKind::Directory, user_id)
            .await?;

        changed_entries.push(ChangedEntry {
            entry_id,
            path: path.as_str().to_string(),
            kind: EntryKind::Directory,
            current_version_id: None,
            change_type: ChangeType::Added,
        });
        let snapshot_entries =
            collect_snapshot_state(&self.entry_repo, namespace_id, Vec::new()).await?;

        finalize_mutation(
            &self.snapshot_repo,
            namespace_id,
            message,
            user_id,
            changed_entries,
            snapshot_entries,
            Vec::new(),
        )
        .await
    }

    /// Copy a resource subtree using a single repository transaction for destination replacement.
    pub async fn copy_entries(
        &self,
        namespace_id: &NamespaceId,
        source: &NormalizedPath,
        destination: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
        overwrite: bool,
    ) -> DomainResult<()> {
        self.copy_entries_with_options(
            namespace_id,
            source,
            destination,
            message,
            user_id,
            CopyOptions {
                overwrite,
                depth_infinity: true,
                condition: None,
                destination_lock_states: Vec::new(),
            },
        )
        .await
    }

    /// Copy a resource and optionally recurse through collection members.
    pub async fn copy_entries_with_options(
        &self,
        namespace_id: &NamespaceId,
        source: &NormalizedPath,
        destination: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
        options: CopyOptions,
    ) -> DomainResult<()> {
        let CopyOptions {
            overwrite,
            depth_infinity,
            condition,
            destination_lock_states,
        } = options;
        let condition = condition.as_ref();
        if source.as_str() == destination.as_str() {
            return Err(DomainError::Conflict {
                message: "Cannot copy a resource onto itself".to_string(),
            });
        }
        // 目标不得在源子树内（✗ 否则 dest 建进 src children = **无限自递归 core dump**
        // r5 实测抓获 ✗ move 同防照抄）
        if destination
            .as_str()
            .starts_with(&format!("{}/", source.as_str()))
        {
            return Err(DomainError::Conflict {
                message: "Cannot copy a resource into its own subtree".to_string(),
            });
        }
        if condition.is_some_and(|condition| {
            condition.namespace_id != *namespace_id || condition.path != *source
        }) {
            return Err(DomainError::PreconditionFailed);
        }
        let mut source_entries = if let Some(condition) = condition {
            self.entry_repo
                .find_subtree_if_current(namespace_id, source, condition)
                .await?
        } else {
            let root = self
                .entry_repo
                .find_by_path(namespace_id, source)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    resource: format!("entry {}", source.as_str()),
                })?;
            if depth_infinity && root.entry_type == EntryKind::Directory {
                self.entry_repo.find_subtree(namespace_id, source).await?
            } else {
                vec![root]
            }
        };
        let src_entry = source_entries
            .iter()
            .find(|entry| entry.path_norm == *source)
            .cloned()
            .ok_or_else(|| DomainError::NotFound {
                resource: format!("entry {}", source.as_str()),
            })?;
        if !depth_infinity || src_entry.entry_type != EntryKind::Directory {
            source_entries.retain(|entry| entry.id == src_entry.id);
        }
        // Complete all predictable validation and source lookups before deleting an
        // overwrite target. Otherwise a malformed source or invalid parent can turn
        // a failed COPY into a destructive operation.
        let dest_parent = {
            let dp = destination.as_str();
            match dp.rfind('/') {
                Some(i) => NormalizedPath::new(&dp[..i]).map_err(|_| DomainError::Validation {
                    message: "Invalid destination parent".to_string(),
                })?,
                None => NormalizedPath::new("").map_err(|_| DomainError::Validation {
                    message: "Invalid destination parent".to_string(),
                })?,
            }
        };
        let parent_entry = self
            .entry_repo
            .find_by_path(namespace_id, &dest_parent)
            .await?;
        let parent_ok = dest_parent.as_str().is_empty()
            || matches!(
                parent_entry.as_ref().map(|e| e.entry_type),
                Some(EntryKind::Directory)
            );
        if !parent_ok {
            return Err(DomainError::Conflict {
                message: "Destination parent is not a collection".to_string(),
            });
        }
        let source_entry_ids = source_entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let source_properties = self
            .entry_repo
            .list_entry_properties(&source_entry_ids)
            .await?;
        let mut source_versions = HashMap::<EntryId, EntryVersion>::new();
        for entry in &source_entries {
            if entry.id == src_entry.id {
                continue;
            }
            if entry.entry_type == EntryKind::File {
                let version_id =
                    entry
                        .current_version_id
                        .as_ref()
                        .ok_or_else(|| DomainError::Validation {
                            message: "Source file has no version".to_string(),
                        })?;
                source_versions.insert(entry.id, self.entry_repo.find_version(version_id).await?);
            }
        }
        if src_entry.entry_type == EntryKind::File {
            let version_id =
                src_entry
                    .current_version_id
                    .as_ref()
                    .ok_or_else(|| DomainError::Validation {
                        message: "Source file has no version".to_string(),
                    })?;
            source_versions.insert(
                src_entry.id,
                self.entry_repo.find_version(version_id).await?,
            );
        }

        let mut copy_entries = Vec::with_capacity(source_entries.len());
        for entry in &source_entries {
            let suffix = entry
                .path_norm
                .as_str()
                .strip_prefix(source.as_str())
                .expect("subtree query only returns source descendants");
            let target_path = NormalizedPath::new(&format!("{}{}", destination.as_str(), suffix))
                .map_err(|_| DomainError::Validation {
                message: "Invalid copied destination path".to_string(),
            })?;
            copy_entries.push(CopyEntrySpec {
                path: target_path,
                entry_type: entry.entry_type,
                version: source_versions.get(&entry.id).cloned(),
                properties: source_properties
                    .get(&entry.id)
                    .cloned()
                    .unwrap_or_default(),
            });
        }
        let (_, orphaned_blobs) = self
            .entry_repo
            .replace_subtree_with_copy(
                namespace_id,
                destination,
                CopySubtreeSpec {
                    entries: &copy_entries,
                    destination_lock_states: &destination_lock_states,
                },
                overwrite,
                user_id,
                message,
            )
            .await?;
        for blob_id in orphaned_blobs {
            if let Err(error) = self.blob_store.delete_blob(&blob_id).await {
                tracing::warn!(%blob_id, %error, "failed to remove blob released by COPY overwrite");
            }
        }
        Ok(())
    }

    pub async fn move_entries(
        &self,
        namespace_id: &NamespaceId,
        sources: &[NormalizedPath],
        destination: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
        // dest 语义（r12 三面评估定案 ✓）：false = 完整目标路径（WebDAV Destination /
        // FTP RNTO ✗ RFC §9.9 dest 即 target）/ true = 容器目录（http 前端拖放 =
        // join(dest, 源名)）——多源强制容器。
        dest_as_container: bool,
    ) -> DomainResult<MutationResult> {
        self.move_entries_with_overwrite(
            namespace_id,
            sources,
            destination,
            message,
            user_id,
            MoveOptions {
                destination_is_container: dest_as_container,
                ..MoveOptions::default()
            },
        )
        .await
    }

    pub async fn move_entry_overwriting(
        &self,
        namespace_id: &NamespaceId,
        source: &NormalizedPath,
        destination: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
        overwrite: bool,
    ) -> DomainResult<MutationResult> {
        self.move_entries_with_overwrite(
            namespace_id,
            std::slice::from_ref(source),
            destination,
            message,
            user_id,
            MoveOptions {
                overwrite_destination: overwrite,
                ..MoveOptions::default()
            },
        )
        .await
    }

    pub async fn move_entry_overwriting_with_condition(
        &self,
        namespace_id: &NamespaceId,
        source: &NormalizedPath,
        destination: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
        options: MoveOptions<'_>,
    ) -> DomainResult<MutationResult> {
        self.move_entries_with_overwrite(
            namespace_id,
            std::slice::from_ref(source),
            destination,
            message,
            user_id,
            options,
        )
        .await
    }

    pub async fn move_entry_with_property_changes(
        &self,
        namespace_id: &NamespaceId,
        source: &NormalizedPath,
        destination: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
        changes: &[vfiles_domain::EntryPropertyChange],
    ) -> DomainResult<MutationResult> {
        self.move_entries_with_overwrite(
            namespace_id,
            std::slice::from_ref(source),
            destination,
            message,
            user_id,
            MoveOptions {
                property_changes: Some(changes),
                ..MoveOptions::default()
            },
        )
        .await
    }

    async fn move_entries_with_overwrite(
        &self,
        namespace_id: &NamespaceId,
        sources: &[NormalizedPath],
        destination: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
        options: MoveOptions<'_>,
    ) -> DomainResult<MutationResult> {
        let MoveOptions {
            destination_is_container: dest_as_container,
            overwrite_destination,
            property_changes,
            condition,
        } = options;
        if sources.is_empty() {
            return Err(DomainError::Validation {
                message: "At least one source path is required".to_string(),
            });
        }

        for (index, source) in sources.iter().enumerate() {
            for other in sources.iter().skip(index + 1) {
                if source.as_str() == other.as_str()
                    || source.as_str().starts_with(&format!("{}/", other.as_str()))
                    || other.as_str().starts_with(&format!("{}/", source.as_str()))
                {
                    return Err(DomainError::Conflict {
                        message: "Source paths must not overlap".to_string(),
                    });
                }
            }
        }

        if sources.len() > 1 && !dest_as_container {
            return Err(DomainError::Validation {
                message: "Multiple sources require a container destination".to_string(),
            });
        }

        let mut moving_entries = Vec::new();
        let mut changed_entries = Vec::new();
        let mut replaced_entries = Vec::new();

        for source in sources {
            let source_entry = self
                .entry_repo
                .find_by_path(namespace_id, source)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    resource: format!("entry {}", source.as_str()),
                })?;

            // r12 dest 语义参数化 ✗ Path = dest 即 target（RFC 完整路径 ✓ 同名目录
            // 覆盖打通 ✗ 原 join 由 dest_目录判定 = 违 RFC 二义（r11 结构债正解））
            let target_root = if dest_as_container {
                join_path(destination, basename(source))?
            } else {
                destination.clone()
            };

            if target_root.as_str() == source.as_str()
                || target_root
                    .as_str()
                    .starts_with(&format!("{}/", source.as_str()))
            {
                return Err(DomainError::Conflict {
                    message: format!(
                        "Cannot move {} into {}",
                        source.as_str(),
                        target_root.as_str()
                    ),
                });
            }

            if overwrite_destination
                && let Some(replaced_root) = self
                    .entry_repo
                    .find_by_path(namespace_id, &target_root)
                    .await?
            {
                replaced_entries.extend(
                    collect_descendants(&self.entry_repo, namespace_id, &replaced_root).await?,
                );
            }

            let subtree =
                collect_descendants(&self.entry_repo, namespace_id, &source_entry).await?;
            for entry in subtree {
                let new_path = compute_move_target(source, &entry.path_norm, &target_root)?;
                moving_entries.push((entry, new_path));
            }
        }

        let mut seen_replaced = std::collections::HashSet::new();
        replaced_entries.retain(|entry| seen_replaced.insert(entry.id));
        let replaced_paths: std::collections::HashSet<String> = replaced_entries
            .iter()
            .map(|entry| entry.path_norm.as_str().to_string())
            .collect();

        let original_paths = moving_entries
            .iter()
            .map(|(entry, _)| entry.path_norm.as_str().to_string())
            .collect::<std::collections::HashSet<String>>();

        let candidates = moving_entries
            .iter()
            .filter(|(_, new_path)| {
                !original_paths.contains(new_path.as_str())
                    && !replaced_paths.contains(new_path.as_str())
            })
            .map(|(_, new_path)| new_path.clone())
            .collect::<Vec<_>>();

        // 一次批量查询待检查的新路径，避免逐个 find_by_path。
        if let Some(existing) = self
            .entry_repo
            .find_paths(namespace_id, &candidates)
            .await?
            .into_iter()
            .next()
        {
            return Err(DomainError::PathConflict {
                message: format!("Path already exists: {}", existing.path_norm.as_str()),
            });
        }

        moving_entries.sort_by(|left, right| {
            path_depth(&left.0.path_norm).cmp(&path_depth(&right.0.path_norm))
        });

        // 替换删除与源路径改写在同一 EntryRepo 事务提交，避免先删目标后移动失败。
        let moves = moving_entries
            .iter()
            .map(|(entry, new_path)| (entry.id, new_path.clone()))
            .collect::<Vec<_>>();
        let (replaced_entries, blob_refs) = if overwrite_destination {
            self.entry_repo
                .replace_subtree_and_move(namespace_id, destination, &moves, condition)
                .await?
        } else {
            if let Some(changes) = property_changes {
                let source_id = moving_entries
                    .iter()
                    .find(|(entry, _)| entry.path_norm == sources[0])
                    .map(|(entry, _)| entry.id)
                    .ok_or_else(|| DomainError::Internal {
                        message: "Moved source entry is missing".into(),
                    })?;
                self.entry_repo
                    .move_entries_with_property_changes(&moves, &source_id, changes)
                    .await?;
            } else if let Some(condition) = condition {
                self.entry_repo
                    .move_entries_if_current(&moves, condition)
                    .await?;
            } else {
                self.entry_repo.move_entries(&moves).await?;
            }
            (Vec::new(), Vec::new())
        };
        let mut deleted_snapshot_entries = Vec::with_capacity(replaced_entries.len());
        for entry in &replaced_entries {
            changed_entries.push(ChangedEntry {
                entry_id: entry.id,
                path: entry.path_norm.as_str().to_string(),
                kind: entry.entry_type,
                current_version_id: entry.current_version_id,
                change_type: ChangeType::Deleted,
            });
            deleted_snapshot_entries.push(pending_snapshot_entry(
                entry.id,
                &entry.path_norm,
                entry.entry_type,
                None,
                ChangeType::Deleted,
            ));
        }

        for (entry, new_path) in &moving_entries {
            changed_entries.push(ChangedEntry {
                entry_id: entry.id,
                path: new_path.as_str().to_string(),
                kind: entry.entry_type,
                current_version_id: entry.current_version_id,
                change_type: ChangeType::Renamed,
            });
        }
        let mut cleanup_warnings = Vec::new();
        let released_blobs = self.entry_repo.release_blob_references(&blob_refs).await?;
        for blob_id in released_blobs {
            if let Err(err) = self.blob_store.delete_blob(&blob_id).await {
                cleanup_warnings.push(format!(
                    "Failed to delete unreferenced blob {}: {}",
                    blob_id, err
                ));
            }
        }
        let mut snapshot_entries =
            collect_snapshot_state(&self.entry_repo, namespace_id, deleted_snapshot_entries)
                .await?;
        let rename_changes: HashMap<EntryId, ChangeType> = changed_entries
            .iter()
            .filter(|entry| entry.change_type == ChangeType::Renamed)
            .map(|entry| (entry.entry_id, entry.change_type))
            .collect();
        for entry in &mut snapshot_entries {
            if let Some(change_type) = rename_changes.get(&entry.entry_id) {
                entry.change_type = *change_type;
            }
        }

        finalize_mutation(
            &self.snapshot_repo,
            namespace_id,
            message,
            user_id,
            changed_entries,
            snapshot_entries,
            cleanup_warnings,
        )
        .await
    }

    pub async fn delete_entries(
        &self,
        namespace_id: &NamespaceId,
        paths: &[NormalizedPath],
        message: Option<&str>,
        user_id: &UserId,
    ) -> DomainResult<MutationResult> {
        self.delete_entries_inner(namespace_id, paths, message, user_id, None)
            .await
    }

    pub async fn delete_entries_with_condition(
        &self,
        namespace_id: &NamespaceId,
        paths: &[NormalizedPath],
        message: Option<&str>,
        user_id: &UserId,
        condition: &vfiles_domain::EntryWriteCondition,
    ) -> DomainResult<MutationResult> {
        self.delete_entries_inner(namespace_id, paths, message, user_id, Some(condition))
            .await
    }

    async fn delete_entries_inner(
        &self,
        namespace_id: &NamespaceId,
        paths: &[NormalizedPath],
        message: Option<&str>,
        user_id: &UserId,
        condition: Option<&vfiles_domain::EntryWriteCondition>,
    ) -> DomainResult<MutationResult> {
        if paths.is_empty() {
            return Err(DomainError::Validation {
                message: "At least one path is required".to_string(),
            });
        }

        let mut all_entries = Vec::new();
        for path in paths {
            let entry = self
                .entry_repo
                .find_by_path(namespace_id, path)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    resource: format!("entry {}", path.as_str()),
                })?;
            all_entries.extend(collect_descendants(&self.entry_repo, namespace_id, &entry).await?);
        }

        let mut seen_entries = std::collections::HashSet::new();
        all_entries.retain(|entry| seen_entries.insert(entry.id));

        all_entries.sort_by_key(|entry| std::cmp::Reverse(path_depth(&entry.path_norm)));

        let changed_entries = all_entries
            .iter()
            .map(|entry| ChangedEntry {
                entry_id: entry.id,
                path: entry.path_norm.as_str().to_string(),
                kind: entry.entry_type,
                current_version_id: entry.current_version_id,
                change_type: ChangeType::Deleted,
            })
            .collect::<Vec<_>>();
        let mut deleted_entries = Vec::with_capacity(all_entries.len());
        let mut cleanup_warnings = Vec::new();

        // 删除类型的快照项会忽略版本字段，无需逐条查询当前版本。
        for entry in &all_entries {
            deleted_entries.push(pending_snapshot_entry(
                entry.id,
                &entry.path_norm,
                entry.entry_type,
                None,
                ChangeType::Deleted,
            ));
        }

        // 一次汇总所有被删条目的版本引用，再批量删除并统一释放。
        let entry_ids: Vec<EntryId> = all_entries.iter().map(|entry| entry.id).collect();
        let mut blob_counts = HashMap::<BlobId, u32>::new();
        for version in self
            .entry_repo
            .find_versions_for_entries(&entry_ids)
            .await?
        {
            if let Some(blob_id) = version.blob_id {
                *blob_counts.entry(blob_id).or_insert(0) += 1;
            }
        }
        let blob_refs = blob_counts.into_iter().collect::<Vec<_>>();

        if let Some(condition) = condition {
            self.entry_repo
                .delete_entries_if_current(&entry_ids, condition)
                .await?;
        } else {
            self.entry_repo.delete_entries(&entry_ids).await?;
        }

        let released_blobs = self.entry_repo.release_blob_references(&blob_refs).await?;
        for blob_id in released_blobs {
            if let Err(err) = self.blob_store.delete_blob(&blob_id).await {
                cleanup_warnings.push(format!(
                    "Failed to delete unreferenced blob {}: {}",
                    blob_id, err
                ));
            }
        }

        let snapshot_entries =
            collect_snapshot_state(&self.entry_repo, namespace_id, deleted_entries).await?;

        finalize_mutation(
            &self.snapshot_repo,
            namespace_id,
            message,
            user_id,
            changed_entries,
            snapshot_entries,
            cleanup_warnings,
        )
        .await
    }
}

#[derive(Debug, Clone)]
pub struct UploadSessionView {
    pub upload_id: UploadId,
    pub target_path: String,
    pub chunk_size: u64,
    pub total_parts: u32,
    pub received_parts: Vec<u32>,
    pub missing_parts: Vec<u32>,
    pub state: UploadState,
    pub expires_at: time::OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct UploadCompleteResponse {
    pub upload: UploadSession,
    pub entry: Entry,
    pub version: EntryVersion,
    /// Snapshot creation is auxiliary to the committed object write and may fail independently.
    pub mutation: Option<MutationResult>,
}

#[derive(Debug, Clone)]
pub struct UploadService<E, S, B, U> {
    entry_repo: E,
    snapshot_repo: S,
    blob_store: B,
    upload_store: U,
}

impl<E, S, B, U> UploadService<E, S, B, U>
where
    E: EntryRepo + Send + Sync,
    S: SnapshotRepo + Send + Sync,
    B: BlobStore + Send + Sync,
    U: UploadStore,
{
    pub fn new(entry_repo: E, snapshot_repo: S, blob_store: B, upload_store: U) -> Self {
        Self {
            entry_repo,
            snapshot_repo,
            blob_store,
            upload_store,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn init_upload(
        &self,
        namespace_id: &NamespaceId,
        target_path: &NormalizedPath,
        filename: &str,
        size_bytes: u64,
        mime_type: Option<&str>,
        requested_chunk_size: Option<u64>,
        user_id: &UserId,
    ) -> DomainResult<UploadSessionView> {
        validate_filename(filename)?;

        if size_bytes > 0 {
            // No extra validation needed here, but keep chunk math explicit.
        }

        let file_path = uploaded_file_path(target_path, filename)?;
        if let Some(entry) = self
            .entry_repo
            .find_by_path(namespace_id, &file_path)
            .await?
            && entry.entry_type != EntryKind::File
        {
            return Err(DomainError::PathConflict {
                message: format!("Path is occupied by a directory: {}", file_path.as_str()),
            });
        }

        let chunk_size =
            requested_chunk_size.unwrap_or_else(|| size_bytes.clamp(1, 5 * 1024 * 1024));
        if chunk_size == 0 {
            return Err(DomainError::Validation {
                message: "Chunk size must be greater than zero".to_string(),
            });
        }
        let upload_id = self
            .upload_store
            .create_upload_session(
                namespace_id,
                target_path,
                filename,
                mime_type,
                size_bytes,
                chunk_size,
                user_id,
            )
            .await?;

        self.get_upload_status(&upload_id).await
    }

    /// 创建声明总长度未知的流式上传会话。
    pub async fn init_stream_upload_unknown_size(
        &self,
        namespace_id: &NamespaceId,
        target_path: &NormalizedPath,
        filename: &str,
        mime_type: Option<&str>,
        user_id: &UserId,
    ) -> DomainResult<UploadSessionView> {
        self.init_upload(
            namespace_id,
            target_path,
            filename,
            0,
            mime_type,
            Some(1),
            user_id,
        )
        .await
    }

    pub async fn upload_part(
        &self,
        upload_id: &UploadId,
        part_index: u32,
        data: &[u8],
    ) -> DomainResult<u64> {
        use sha2::{Digest, Sha256};

        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }
        if matches!(session.state, UploadState::Completed | UploadState::Failed) {
            return Err(DomainError::UploadConflict);
        }
        if session.total_chunks > 0 && part_index >= session.total_chunks {
            return Err(DomainError::UploadPartInvalid);
        }

        let mut hasher = Sha256::new();
        hasher.update(data);
        let sha256 = hex::encode(hasher.finalize());

        self.upload_store
            .store_upload_part(upload_id, part_index, data, &sha256)
            .await?;

        Ok(data.len() as u64)
    }

    /// 流式接收上传分片，避免 S3 大分片在应用层聚合到内存。
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_part_from_stream(
        &self,
        upload_id: &UploadId,
        part_index: u32,
        expected_size: Option<u64>,
        max_size: Option<u64>,
        expected_md5: Option<[u8; 16]>,
        expected_sha256: Option<[u8; 32]>,
        expected_crc32: Option<u32>,
        expected_crc32c: Option<u32>,
        expected_crc64nvme: Option<u64>,
        expected_sha1: Option<[u8; 20]>,
        reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> DomainResult<UploadPartReceipt> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }
        if matches!(session.state, UploadState::Completed | UploadState::Failed) {
            return Err(DomainError::UploadConflict);
        }
        if session.total_chunks > 0 && part_index >= session.total_chunks {
            return Err(DomainError::UploadPartInvalid);
        }

        self.upload_store
            .store_upload_part_stream(
                upload_id,
                part_index,
                expected_size,
                max_size,
                expected_md5,
                expected_sha256,
                expected_crc32,
                expected_crc32c,
                expected_crc64nvme,
                expected_sha1,
                reader,
            )
            .await
    }

    pub async fn complete_upload(
        &self,
        upload_id: &UploadId,
        expected_sha256: Option<&str>,
        message: Option<&str>,
    ) -> DomainResult<UploadCompleteResponse> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }

        let upload_stream = self.upload_store.assemble_upload_stream(upload_id).await?;
        self.commit_upload_stream(
            session,
            upload_stream,
            expected_sha256,
            None,
            None,
            None,
            None,
            None,
            message,
            true,
            None,
            None,
        )
        .await
    }

    pub async fn complete_upload_from_stream(
        &self,
        upload_id: &UploadId,
        expected_sha256: Option<&str>,
        message: Option<&str>,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> DomainResult<UploadCompleteResponse> {
        self.complete_upload_from_stream_with_md5(
            upload_id,
            expected_sha256,
            None,
            None,
            None,
            None,
            None,
            message,
            upload_stream,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn complete_upload_from_stream_with_md5(
        &self,
        upload_id: &UploadId,
        expected_sha256: Option<&str>,
        expected_md5: Option<[u8; 16]>,
        expected_crc32: Option<u32>,
        expected_crc32c: Option<u32>,
        expected_crc64nvme: Option<u64>,
        expected_sha1: Option<[u8; 20]>,
        message: Option<&str>,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> DomainResult<UploadCompleteResponse> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }

        self.commit_upload_stream(
            session,
            upload_stream,
            expected_sha256,
            expected_md5,
            expected_crc32,
            expected_crc32c,
            expected_crc64nvme,
            expected_sha1,
            message,
            true,
            None,
            None,
        )
        .await
    }

    /// 完成总长度未知的流式上传（S3 chunked PUT 等传输场景）。
    pub async fn complete_upload_from_stream_unknown_size(
        &self,
        upload_id: &UploadId,
        expected_sha256: Option<&str>,
        message: Option<&str>,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> DomainResult<UploadCompleteResponse> {
        self.complete_upload_from_stream_unknown_size_with_md5(
            upload_id,
            expected_sha256,
            None,
            None,
            None,
            None,
            None,
            message,
            upload_stream,
        )
        .await
    }

    pub async fn complete_upload_from_stream_unknown_size_with_condition(
        &self,
        upload_id: &UploadId,
        message: Option<&str>,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        condition: &vfiles_domain::EntryWriteCondition,
    ) -> DomainResult<UploadCompleteResponse> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }

        self.commit_upload_stream(
            session,
            upload_stream,
            None,
            None,
            None,
            None,
            None,
            None,
            message,
            false,
            None,
            Some(condition),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn complete_upload_from_stream_unknown_size_with_md5(
        &self,
        upload_id: &UploadId,
        expected_sha256: Option<&str>,
        expected_md5: Option<[u8; 16]>,
        expected_crc32: Option<u32>,
        expected_crc32c: Option<u32>,
        expected_crc64nvme: Option<u64>,
        expected_sha1: Option<[u8; 20]>,
        message: Option<&str>,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> DomainResult<UploadCompleteResponse> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }

        self.commit_upload_stream(
            session,
            upload_stream,
            expected_sha256,
            expected_md5,
            expected_crc32,
            expected_crc32c,
            expected_crc64nvme,
            expected_sha1,
            message,
            false,
            None,
            None,
        )
        .await
    }

    /// Complete an upload while committing version-derived entry properties in the same
    /// repository transaction as the new version.
    #[allow(clippy::too_many_arguments)]
    pub async fn complete_upload_from_stream_with_properties(
        &self,
        upload_id: &UploadId,
        expected_sha256: Option<&str>,
        expected_md5: Option<[u8; 16]>,
        expected_crc32: Option<u32>,
        expected_crc32c: Option<u32>,
        expected_crc64nvme: Option<u64>,
        expected_sha1: Option<[u8; 20]>,
        message: Option<&str>,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        enforce_size: bool,
        properties: &(
             dyn Fn(vfiles_domain::VersionId) -> Vec<vfiles_domain::EntryPropertyChange>
                 + Send
                 + Sync
         ),
    ) -> DomainResult<UploadCompleteResponse> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }
        self.commit_upload_stream(
            session,
            upload_stream,
            expected_sha256,
            expected_md5,
            expected_crc32,
            expected_crc32c,
            expected_crc64nvme,
            expected_sha1,
            message,
            enforce_size,
            Some(properties),
            None,
        )
        .await
    }

    /// S3 multipart：开启**未知总大小**的上传会话（`declared_size=0` / `total_chunks=0`）；
    /// part 按号存储，完成时由调用方指定需拼接的 part 索引。S3 层映射为 `partNumber-1`。
    pub async fn init_multipart_upload(
        &self,
        namespace_id: &NamespaceId,
        target_path: &NormalizedPath,
        filename: &str,
        mime_type: Option<&str>,
        user_id: &UserId,
    ) -> DomainResult<UploadSessionView> {
        self.init_upload(
            namespace_id,
            target_path,
            filename,
            0,
            mime_type,
            Some(1),
            user_id,
        )
        .await
    }

    /// S3 multipart：完成（**跳过量校验** ✗ 总大小在 CreateMultipartUpload 时未知）。
    pub async fn complete_multipart_upload(
        &self,
        upload_id: &UploadId,
        part_indices: &[u32],
        message: Option<&str>,
    ) -> DomainResult<UploadCompleteResponse> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }
        let upload_stream = self
            .upload_store
            .assemble_upload_stream_parts(upload_id, part_indices)
            .await?;
        self.commit_upload_stream(
            session,
            upload_stream,
            None,
            None,
            None,
            None,
            None,
            None,
            message,
            false,
            None,
            None,
        )
        .await
    }

    pub async fn complete_multipart_upload_with_properties(
        &self,
        upload_id: &UploadId,
        part_indices: &[u32],
        message: Option<&str>,
        properties: &(
             dyn Fn(vfiles_domain::VersionId) -> Vec<vfiles_domain::EntryPropertyChange>
                 + Send
                 + Sync
         ),
    ) -> DomainResult<UploadCompleteResponse> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }
        let upload_stream = self
            .upload_store
            .assemble_upload_stream_parts(upload_id, part_indices)
            .await?;
        self.commit_upload_stream(
            session,
            upload_stream,
            None,
            None,
            None,
            None,
            None,
            None,
            message,
            false,
            Some(properties),
            None,
        )
        .await
    }

    /// 写会话自定义元数据（S3 `x-amz-meta-*`）。
    pub async fn set_upload_custom_metadata(
        &self,
        upload_id: &UploadId,
        metadata: &std::collections::BTreeMap<String, String>,
    ) -> DomainResult<()> {
        self.upload_store
            .set_upload_custom_metadata(upload_id, metadata)
            .await
    }

    /// 读会话自定义元数据（无则空表）。
    pub async fn get_upload_custom_metadata(
        &self,
        upload_id: &UploadId,
    ) -> DomainResult<std::collections::BTreeMap<String, String>> {
        self.upload_store
            .get_upload_custom_metadata(upload_id)
            .await
    }

    /// 列出本命名空间的上传会话（S3 `ListMultipartUploads` 用）。
    pub async fn list_upload_sessions(
        &self,
        namespace_id: &NamespaceId,
    ) -> DomainResult<Vec<vfiles_domain::UploadSession>> {
        Ok(self
            .upload_store
            .list_upload_sessions()
            .await?
            .into_iter()
            .filter(|s| &s.namespace_id == namespace_id)
            .collect())
    }

    /// 分页列出命名空间下进行中的 S3 multipart 会话。
    pub async fn list_upload_sessions_page(
        &self,
        namespace_id: &NamespaceId,
        prefix: &str,
        delimiter: Option<&str>,
        after_key: Option<&str>,
        after_upload_id: Option<&str>,
        limit: u32,
    ) -> DomainResult<Vec<vfiles_domain::UploadSessionListItem>>
    where
        U: Sync,
    {
        self.upload_store
            .list_upload_sessions_page(
                namespace_id,
                prefix,
                delimiter,
                after_key,
                after_upload_id,
                limit,
            )
            .await
    }

    /// 读单个分片内容（S3 `ListParts` ETag / 完成校验用）。
    pub async fn read_upload_part(
        &self,
        upload_id: &UploadId,
        part_index: u32,
    ) -> DomainResult<Option<Vec<u8>>> {
        self.upload_store
            .read_upload_part(upload_id, part_index)
            .await
    }

    pub async fn open_upload_part(
        &self,
        upload_id: &UploadId,
        part_index: u32,
    ) -> DomainResult<Option<(Box<dyn tokio::io::AsyncRead + Send + Unpin>, u64)>> {
        self.upload_store
            .open_upload_part(upload_id, part_index)
            .await
    }

    /// 列出已接收的 part（S3 `ListParts` / 完成校验用 ✗ 含 size）。
    pub async fn list_upload_parts(
        &self,
        upload_id: &UploadId,
    ) -> DomainResult<Vec<vfiles_domain::UploadPart>> {
        self.upload_store.get_upload_parts(upload_id).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit_upload_stream(
        &self,
        session: UploadSession,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        expected_sha256: Option<&str>,
        expected_md5: Option<[u8; 16]>,
        expected_crc32: Option<u32>,
        expected_crc32c: Option<u32>,
        expected_crc64nvme: Option<u64>,
        expected_sha1: Option<[u8; 20]>,
        message: Option<&str>,
        enforce_size: bool,
        version_properties: Option<
            &(
                 dyn Fn(vfiles_domain::VersionId) -> Vec<vfiles_domain::EntryPropertyChange>
                     + Send
                     + Sync
             ),
        >,
        condition: Option<&vfiles_domain::EntryWriteCondition>,
    ) -> DomainResult<UploadCompleteResponse> {
        let (blob_id, content_hash, created_blob, stored_size) = self
            .blob_store
            .store_blob_stream(
                upload_stream,
                expected_sha256,
                expected_md5,
                expected_crc32,
                expected_crc32c,
                expected_crc64nvme,
                expected_sha1,
            )
            .await?;

        if enforce_size && stored_size != session.declared_size.as_u64() {
            if created_blob {
                let _ = self.blob_store.delete_blob(&blob_id).await;
            }
            return Err(DomainError::UploadConflict);
        }

        let mut created_entry_id = None;
        let prepare_result = async {
            let file_path = uploaded_file_path(&session.target_path_norm, &session.filename)?;
            let changed_entries = ensure_directory_path(
                &self.entry_repo,
                &session.namespace_id,
                &session.target_path_norm,
                &session.owner_user_id,
            )
            .await?;
            let entry = if let Some(entry) = self
                .entry_repo
                .find_by_path(&session.namespace_id, &file_path)
                .await?
            {
                if entry.entry_type != EntryKind::File {
                    return Err(DomainError::PathConflict {
                        message: format!("Path is occupied by a directory: {}", file_path.as_str()),
                    });
                }
                entry
            } else {
                let entry_id = self
                    .entry_repo
                    .create_entry(
                        &session.namespace_id,
                        &file_path,
                        EntryKind::File,
                        &session.owner_user_id,
                    )
                    .await?;
                created_entry_id = Some(entry_id);
                self.entry_repo.find_by_id(&entry_id).await?
            };
            Ok::<_, DomainError>((file_path, changed_entries, entry))
        }
        .await;
        let (file_path, mut changed_entries, mut entry) = match prepare_result {
            Ok(prepared) => prepared,
            Err(error) => {
                if let Some(entry_id) = created_entry_id
                    && let Err(cleanup_error) = self.entry_repo.delete_entry(&entry_id).await
                {
                    tracing::warn!(%cleanup_error, %entry_id, "failed to rollback empty upload entry");
                }
                if created_blob
                    && let Err(cleanup_error) = self.blob_store.delete_blob(&blob_id).await
                {
                    tracing::warn!(%cleanup_error, %blob_id, "failed to rollback unpublished upload blob");
                }
                return Err(error);
            }
        };
        let entry_id = entry.id;

        let mime_type = session
            .mime_type
            .clone()
            .or_else(|| guess_mime_type(&session.filename));
        let normalized_message = normalize_message(message);
        let no_properties = |_version_id| Vec::new();
        let properties = version_properties.unwrap_or(&no_properties);
        let mut effective_condition = condition.cloned();
        if let Some(condition) = effective_condition.as_mut()
            && condition.expected_entry_id.is_none()
            && created_entry_id == Some(entry_id)
        {
            // This upload created the entry after observing absence. Bind the condition to
            // that new entry so a concurrent creator cannot be mistaken for our own entry.
            condition.expected_entry_id = Some(entry_id);
        }
        let version_result = self
            .entry_repo
            .create_version_with_properties(
                &entry_id,
                Some(&blob_id),
                Some(&content_hash),
                stored_size,
                mime_type.as_deref(),
                &session.owner_user_id,
                normalized_message.as_deref(),
                properties,
                effective_condition.as_ref(),
            )
            .await;
        let version = match version_result {
            Ok(version) => version,
            Err(err) => {
                if let Some(entry_id) = created_entry_id
                    && let Err(cleanup_error) = self.entry_repo.delete_entry(&entry_id).await
                {
                    tracing::warn!(%cleanup_error, %entry_id, "failed to rollback empty upload entry");
                }
                if created_blob
                    && let Err(cleanup_error) = self.blob_store.delete_blob(&blob_id).await
                {
                    tracing::warn!(%cleanup_error, %blob_id, "failed to rollback unpublished upload blob");
                }
                return Err(err);
            }
        };

        let mut upload = session.clone();
        upload.state = UploadState::Completed;
        let completed_at = time::OffsetDateTime::now_utc();
        upload.completed_at = Some(completed_at);
        upload.updated_at = completed_at;
        if let Err(err) = self.upload_store.complete_upload_session(&session.id).await {
            // The object version is already committed. Returning an error here
            // would tell the client to retry a write that has in fact succeeded.
            tracing::warn!(
                upload_id = %session.id,
                error = ?err,
                "failed to mark committed upload session complete"
            );
        }
        if let Err(err) = self.upload_store.cancel_upload_session(&session.id).await {
            tracing::warn!(
                upload_id = %session.id,
                error = ?err,
                "failed to cleanup completed upload session"
            );
        }
        entry.current_version_id = Some(version.id);

        changed_entries.push(ChangedEntry {
            entry_id,
            path: file_path.as_str().to_string(),
            kind: EntryKind::File,
            current_version_id: Some(version.id),
            change_type: if version.version_no == 1 {
                ChangeType::Added
            } else {
                ChangeType::Modified
            },
        });

        let mutation_result = async {
            let snapshot_entries =
                collect_snapshot_state(&self.entry_repo, &session.namespace_id, Vec::new()).await?;
            finalize_mutation(
                &self.snapshot_repo,
                &session.namespace_id,
                message,
                &session.owner_user_id,
                changed_entries,
                snapshot_entries,
                Vec::new(),
            )
            .await
        }
        .await;
        let mutation = match mutation_result {
            Ok(mutation) => Some(mutation),
            Err(error) => {
                tracing::error!(
                    upload_id = %session.id,
                    error = ?error,
                    "object version committed but snapshot finalization failed"
                );
                None
            }
        };

        Ok(UploadCompleteResponse {
            upload,
            entry,
            version,
            mutation,
        })
    }

    pub async fn cancel_upload(&self, upload_id: &UploadId) -> DomainResult<()> {
        self.upload_store.cancel_upload_session(upload_id).await
    }

    pub async fn get_upload_status(&self, upload_id: &UploadId) -> DomainResult<UploadSessionView> {
        let upload = self.upload_store.get_upload_session(upload_id).await?;
        let parts = self.upload_store.get_upload_parts(upload_id).await?;
        Ok(upload_view_from_session(&upload, &parts))
    }

    /// 读取上传会话实体，供协议适配层校验会话归属与目标路径。
    pub async fn get_upload_session(
        &self,
        upload_id: &UploadId,
    ) -> DomainResult<vfiles_domain::UploadSession> {
        self.upload_store.get_upload_session(upload_id).await
    }
}

#[derive(Debug, Clone)]
pub struct VersionSummary {
    pub version_id: VersionId,
    pub version_no: u32,
    pub created_at: time::OffsetDateTime,
    pub actor_name: String,
    pub message: Option<String>,
    pub change_type: ChangeType,
    pub has_custom_message: bool,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
    pub is_current: bool,
    pub restorable: bool,
    pub diffable: bool,
}

#[derive(Debug, Clone)]
pub struct EntryHistoryPage {
    pub path: String,
    pub entry_id: EntryId,
    pub current_version_id: Option<VersionId>,
    pub items: Vec<VersionSummary>,
    pub next_cursor: Option<String>,
    pub total_items: usize,
}

#[derive(Debug, Clone)]
pub struct SnapshotSummary {
    pub snapshot_id: SnapshotId,
    pub created_at: time::OffsetDateTime,
    pub actor_name: String,
    pub message: Option<String>,
    pub kind: SnapshotKind,
}

#[derive(Debug, Clone)]
pub struct DirectoryHistoryPage {
    pub path: String,
    pub current_snapshot_id: Option<SnapshotId>,
    pub items: Vec<SnapshotSummary>,
    pub total_items: usize,
}

#[derive(Debug, Clone)]
pub struct HistoryService<E, S, B, U> {
    entry_repo: E,
    snapshot_repo: S,
    blob_store: B,
    user_repo: U,
}

impl<E, S, B, U> HistoryService<E, S, B, U>
where
    E: EntryRepo + Send + Sync,
    S: SnapshotRepo + Send + Sync,
    B: BlobStore + Send + Sync,
    U: UserRepo,
{
    pub fn new(entry_repo: E, snapshot_repo: S, blob_store: B, user_repo: U) -> Self {
        Self {
            entry_repo,
            snapshot_repo,
            blob_store,
            user_repo,
        }
    }

    async fn resolve_actor_names<I>(&self, user_ids: I) -> HashMap<UserId, String>
    where
        I: IntoIterator<Item = UserId>,
    {
        let mut names = HashMap::new();

        for user_id in user_ids {
            if names.contains_key(&user_id) {
                continue;
            }

            let actor_name = self
                .user_repo
                .find_by_id(&user_id)
                .await
                .map(|user| user.username.to_string())
                .unwrap_or_else(|_| user_id.to_string());

            names.insert(user_id, actor_name);
        }

        names
    }

    async fn load_blob_bytes(&self, blob_id: &BlobId) -> DomainResult<Vec<u8>> {
        self.blob_store
            .get_blob(blob_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "blob data".to_string(),
            })
    }

    pub async fn entry_history(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        cursor: Option<&str>,
        limit: u32,
    ) -> DomainResult<EntryHistoryPage> {
        let entry = self
            .entry_repo
            .find_by_path(namespace_id, path)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "entry".to_string(),
            })?;

        if entry.entry_type != EntryKind::File {
            return Err(DomainError::Validation {
                message: "History is only available for files".to_string(),
            });
        }

        let versions = self
            .entry_repo
            .get_entry_history(&entry.id, u32::MAX, None)
            .await?;

        let start_index = if let Some(cursor) = cursor {
            let cursor_id =
                VersionId::from_string(cursor).map_err(|_| DomainError::Validation {
                    message: "Invalid history cursor".to_string(),
                })?;
            versions
                .iter()
                .position(|version| version.id == cursor_id)
                .map(|index| index + 1)
                .ok_or_else(|| DomainError::Validation {
                    message: "Unknown history cursor".to_string(),
                })?
        } else {
            0
        };

        let effective_limit = limit.max(1) as usize;
        let page_versions = versions
            .iter()
            .skip(start_index)
            .take(effective_limit)
            .cloned()
            .collect::<Vec<_>>();
        let actor_ids = page_versions
            .iter()
            .map(|version| version.created_by)
            .collect::<Vec<_>>();
        let actor_names = self.resolve_actor_names(actor_ids).await;
        let next_cursor = if start_index + page_versions.len() < versions.len() {
            page_versions.last().map(|version| version.id.to_string())
        } else {
            None
        };

        Ok(EntryHistoryPage {
            path: path.as_str().to_string(),
            entry_id: entry.id,
            current_version_id: entry.current_version_id,
            items: page_versions
                .into_iter()
                .map(|version| VersionSummary {
                    version_id: version.id,
                    version_no: version.version_no,
                    created_at: version.created_at,
                    actor_name: actor_names
                        .get(&version.created_by)
                        .cloned()
                        .unwrap_or_else(|| version.created_by.to_string()),
                    message: message_for(&version),
                    change_type: version.change_type,
                    has_custom_message: version.change_message.is_some(),
                    size_bytes: version.size_bytes.as_u64(),
                    mime_type: version.mime_type.clone(),
                    is_current: entry.current_version_id == Some(version.id),
                    restorable: entry.current_version_id != Some(version.id),
                    diffable: version.is_text,
                })
                .collect(),
            next_cursor,
            total_items: versions.len(),
        })
    }

    pub async fn directory_history(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        limit: u32,
    ) -> DomainResult<DirectoryHistoryPage> {
        let snapshots = self
            .snapshot_repo
            .list_snapshots(namespace_id, u32::MAX, None)
            .await?;
        let actor_ids = snapshots
            .iter()
            .map(|snapshot| snapshot.created_by)
            .collect::<Vec<_>>();
        let actor_names = self.resolve_actor_names(actor_ids).await;
        let mut items = Vec::new();

        for snapshot in snapshots {
            let is_relevant = if path.as_str().is_empty() {
                true
            } else {
                self.snapshot_repo
                    .get_snapshot_entries(&snapshot.id)
                    .await?
                    .iter()
                    .any(|entry| path_matches_scope(&entry.entry_path, path))
            };

            if !is_relevant {
                continue;
            }

            items.push(SnapshotSummary {
                snapshot_id: snapshot.id,
                created_at: snapshot.created_at,
                actor_name: actor_names
                    .get(&snapshot.created_by)
                    .cloned()
                    .unwrap_or_else(|| snapshot.created_by.to_string()),
                message: snapshot
                    .message
                    .as_ref()
                    .map(|value| value.as_str().to_string()),
                kind: snapshot.kind,
            });
        }

        let total_items = items.len();
        items.truncate(limit.max(1) as usize);

        Ok(DirectoryHistoryPage {
            path: path.as_str().to_string(),
            current_snapshot_id: items.first().map(|item| item.snapshot_id),
            items,
            total_items,
        })
    }

    pub async fn diff_entry(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        commit: &VersionId,
        parent: Option<&VersionId>,
    ) -> DomainResult<String> {
        let entry = self
            .entry_repo
            .find_by_path(namespace_id, path)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "file version".to_string(),
            })?;
        if entry.entry_type != EntryKind::File {
            return Err(DomainError::NotFound {
                resource: "file version".to_string(),
            });
        }

        let versions = self
            .entry_repo
            .get_entry_history(&entry.id, u32::MAX, None)
            .await?;
        let current_index = versions
            .iter()
            .position(|version| version.id == *commit)
            .ok_or(DomainError::VersionNotFound)?;
        let current = versions[current_index].clone();

        if !is_text_mime_type(current.mime_type.as_deref()) {
            return Err(DomainError::Validation {
                message: "Only text files can be diffed".to_string(),
            });
        }

        let previous = if let Some(parent) = parent {
            Some(
                versions
                    .iter()
                    .find(|version| version.id == *parent)
                    .cloned()
                    .ok_or(DomainError::VersionNotFound)?,
            )
        } else {
            versions.get(current_index + 1).cloned()
        };

        let current_blob = current.blob_id.ok_or_else(|| DomainError::NotFound {
            resource: "blob".to_string(),
        })?;
        let current_bytes = self.load_blob_bytes(&current_blob).await?;
        let previous_bytes = if let Some(previous) = previous.as_ref() {
            if let Some(blob_id) = previous.blob_id {
                self.load_blob_bytes(&blob_id).await?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let previous_text = String::from_utf8_lossy(&previous_bytes);
        let current_text = String::from_utf8_lossy(&current_bytes);
        Ok(
            similar::TextDiff::from_lines(previous_text.as_ref(), current_text.as_ref())
                .unified_diff()
                .context_radius(3)
                .header(
                    previous
                        .as_ref()
                        .map(|value| value.id.to_string())
                        .as_deref()
                        .unwrap_or("empty"),
                    &current.id.to_string(),
                )
                .to_string(),
        )
    }

    pub async fn restore_version(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        version_id: &VersionId,
        message: Option<&str>,
        user_id: &UserId,
    ) -> DomainResult<MutationResult> {
        let entry = self
            .entry_repo
            .find_by_path(namespace_id, path)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "entry".to_string(),
            })?;

        if entry.entry_type != EntryKind::File {
            return Err(DomainError::Validation {
                message: "Only file versions can be restored".to_string(),
            });
        }

        let version = self.entry_repo.find_version(version_id).await?;
        if version.entry_id != entry.id {
            return Err(DomainError::VersionNotFound);
        }

        let restored = self
            .entry_repo
            .create_version(
                &entry.id,
                version.blob_id.as_ref(),
                version.blob_id.as_ref().map(|_| &version.content_hash),
                version.size_bytes.as_u64(),
                version.mime_type.as_deref(),
                user_id,
                normalize_message(message)
                    .or_else(|| Some(format!("Restore version {}", version.version_no)))
                    .as_deref(),
            )
            .await?;
        let snapshot_entries =
            collect_snapshot_state(&self.entry_repo, namespace_id, Vec::new()).await?;

        finalize_mutation(
            &self.snapshot_repo,
            namespace_id,
            message,
            user_id,
            vec![ChangedEntry {
                entry_id: entry.id,
                path: path.as_str().to_string(),
                kind: EntryKind::File,
                current_version_id: Some(restored.id),
                change_type: ChangeType::Modified,
            }],
            snapshot_entries,
            Vec::new(),
        )
        .await
    }
}

#[derive(Debug, Clone)]
pub struct MutationResult {
    pub snapshot_id: SnapshotId,
    pub changed_entries: Vec<ChangedEntry>,
    pub warnings: Vec<String>,
    pub applied_at: time::OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ChangedEntry {
    pub entry_id: EntryId,
    pub path: String,
    pub kind: EntryKind,
    pub current_version_id: Option<VersionId>,
    pub change_type: ChangeType,
}

/// 单次搜索最多参与分页的结果数；按得分取前 N 条，避免超大结果集进入内存。
pub const MAX_SEARCH_RESULTS: usize = 2000;

#[derive(Debug, Clone)]
pub struct SearchService<R> {
    search_repo: R,
}

impl<R> SearchService<R>
where
    R: SearchRepo + Clone,
{
    pub fn new(search_repo: R) -> Self {
        Self { search_repo }
    }

    /// 合并文件名与内容命中、按得分排序后再分页。
    ///
    /// 分页必须发生在**排序之后**：此前 `LIMIT/OFFSET` 由仓储层按 `created_at`
    /// 各自执行，两路结果合并后被重新按得分排序，导致「下一页」并不是上一页的延续
    /// —— 同时开启文件名与内容搜索时，内容命中会被同页的文件名命中挤出，
    /// 翻页时甚至完全取不到。这里改为仓储层返回全部命中，由本层统一排序 + 切片。
    pub async fn search(&self, query: SearchQuery) -> DomainResult<Vec<SearchResult>> {
        let mut results = Vec::new();

        // Search in entries (filenames and paths)
        if query.search_files {
            let entry_results = self.search_repo.search_entries(&query).await?;
            results.extend(entry_results);
        }

        // Search in content (if enabled and supported)
        if query.search_content {
            let content_results = self.search_repo.search_content(&query).await?;
            results.extend(content_results);
        }

        // Sort by score (descending)，同分时按路径排序，保证分页结果稳定且可复现
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.entry.path_norm.as_str().cmp(b.entry.path_norm.as_str()))
        });

        // 只保留前 MAX_SEARCH_RESULTS 条，避免一次搜索把过多结果带进内存/响应
        if results.len() > MAX_SEARCH_RESULTS {
            results.truncate(MAX_SEARCH_RESULTS);
        }

        // Apply offset/limit after sorting so pages are contiguous
        let offset = query.offset as usize;
        if offset >= results.len() {
            return Ok(Vec::new());
        }

        results = results.split_off(offset);
        if results.len() > query.limit as usize {
            results.truncate(query.limit as usize);
        }

        Ok(results)
    }
}

#[derive(Debug, Clone)]
pub struct ShareService<R, E> {
    share_repo: R,
    entry_repo: E,
}

impl<R, E> ShareService<R, E>
where
    R: ShareRepo + Clone,
    E: EntryRepo + Clone + Send + Sync,
{
    pub fn new(share_repo: R, entry_repo: E) -> Self {
        Self {
            share_repo,
            entry_repo,
        }
    }

    pub async fn create_share(
        &self,
        namespace_id: &NamespaceId,
        entry_path: &NormalizedPath,
        expires_at: Option<time::OffsetDateTime>,
        created_by: &UserId,
    ) -> DomainResult<String> {
        // Find the entry
        let entry = self
            .entry_repo
            .find_by_path(namespace_id, entry_path)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "entry".to_string(),
            })?;

        // Generate a unique share code (simple random string for now)
        let code = self.generate_share_code().await?;

        // Create the share
        let _share_id = self
            .share_repo
            .create_share(
                namespace_id,
                &entry.id,
                entry.current_version_id.as_ref(),
                &code,
                expires_at,
                created_by,
            )
            .await?;

        Ok(code)
    }

    pub async fn get_share(&self, code: &str) -> DomainResult<Share> {
        self.share_repo.find_share_by_code(code).await
    }

    pub async fn access_share(&self, code: &str) -> DomainResult<Share> {
        let share = self.share_repo.find_share_by_code(code).await?;
        self.share_repo.record_share_access(&share.id).await?;
        Ok(share)
    }

    pub async fn list_shares_by_entry(
        &self,
        entry_path: &NormalizedPath,
        namespace_id: &NamespaceId,
    ) -> DomainResult<Vec<Share>> {
        let entry = self
            .entry_repo
            .find_by_path(namespace_id, entry_path)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "entry".to_string(),
            })?;

        self.share_repo.find_shares_by_entry(&entry.id).await
    }

    pub async fn list_shares_by_user(&self, user_id: &UserId) -> DomainResult<Vec<Share>> {
        self.share_repo.find_shares_by_user(user_id).await
    }

    /// 分享管理页使用：附带被分享条目的名称/路径/类型。
    pub async fn list_shares_with_entry_by_user(
        &self,
        user_id: &UserId,
    ) -> DomainResult<Vec<ShareWithEntry>> {
        self.share_repo
            .find_shares_with_entry_by_user(user_id)
            .await
    }

    pub async fn disable_share(&self, code: &str, user_id: &UserId) -> DomainResult<()> {
        // 所有者操作：已过期的链接也必须能找到并停用（否则无法清理过期分享）
        let share = self
            .share_repo
            .find_share_by_code_including_expired(code)
            .await?;

        // Check if user owns this share
        if share.created_by != *user_id {
            return Err(DomainError::Forbidden);
        }

        self.share_repo.disable_share(&share.id).await
    }

    pub async fn cleanup_expired_shares(&self) -> DomainResult<i64> {
        self.share_repo.cleanup_expired_shares().await
    }

    async fn generate_share_code(&self) -> DomainResult<String> {
        loop {
            let code = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();

            // Check if code already exists
            match self.share_repo.find_share_by_code(&code).await {
                Ok(_) => continue,                                    // Code exists, try again
                Err(DomainError::NotFound { .. }) => return Ok(code), // Code is available
                Err(e) => return Err(e),                              // Other error
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdminUserSummary {
    pub id: UserId,
    pub username: String,
    pub email: Option<String>,
    pub role: Role,
    pub disabled: bool,
    pub created_at: time::OffsetDateTime,
    pub last_login: Option<time::OffsetDateTime>,
}

/// 用户实体 → 管理员视角摘要（列表与单查共用，避免字段漂移）。
fn admin_user_summary(user: User) -> AdminUserSummary {
    AdminUserSummary {
        id: user.id,
        username: user.username.as_str().to_string(),
        email: user.email.as_ref().map(|email| email.as_str().to_string()),
        role: user.role,
        disabled: user.disabled,
        created_at: user.created_at,
        last_login: None, // TODO: implement last login tracking
    }
}

#[derive(Debug, Clone)]
pub struct AdminUserList {
    pub users: Vec<AdminUserSummary>,
    pub total_count: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct UpdateUserRequest {
    pub role: Option<Role>,
    pub disabled: Option<bool>,
    pub email: Option<String>,
    /// 改名（修复历史数据里非法用户名时使用）。
    pub username: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdminService<A> {
    admin_repo: A,
    auth_service: AuthService,
}

impl<A> AdminService<A>
where
    A: AdminRepo + Clone,
{
    pub fn new(admin_repo: A, auth_service: AuthService) -> Self {
        Self {
            admin_repo,
            auth_service,
        }
    }

    pub async fn list_users(&self, page: i64, page_size: i64) -> DomainResult<AdminUserList> {
        let offset = (page - 1) * page_size;
        let users = self.admin_repo.list_users(page_size, offset).await?;
        let total_count = self.admin_repo.count_users().await?;

        let user_summaries = users.into_iter().map(admin_user_summary).collect();

        Ok(AdminUserList {
            users: user_summaries,
            total_count,
            page,
            page_size,
        })
    }

    /// 按 ID 查用户（管理员视角，供 CLI 恢复流程展示账号信息）。
    pub async fn find_user(&self, user_id: &UserId) -> DomainResult<AdminUserSummary> {
        self.auth_service
            .user_repo()
            .find_by_id(user_id)
            .await
            .map(admin_user_summary)
    }

    /// 按用户名查用户（忘记账号时先用 `user list` 找到用户名，再据此定位）。
    pub async fn find_user_by_username(
        &self,
        username: &str,
    ) -> DomainResult<Option<AdminUserSummary>> {
        // 历史数据里可能存在未通过校验的用户名；这类账号同样需要能被找到并修复
        let username = Username::new(username).unwrap_or_else(|_| Username::from_stored(username));
        match self
            .auth_service
            .user_repo()
            .find_by_username(&username)
            .await
        {
            Ok(user) => Ok(Some(admin_user_summary(user))),
            Err(DomainError::NotFound { .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub async fn create_user(&self, req: CreateUserRequest) -> DomainResult<UserId> {
        let username = Username::new(&req.username)?;
        let email = EmailAddress::new(&req.email)?;

        // Check if user already exists
        if self
            .auth_service
            .user_repo()
            .find_by_username(&username)
            .await
            .is_ok()
        {
            return Err(DomainError::Conflict {
                message: "Username already exists".to_string(),
            });
        }
        if self
            .auth_service
            .user_repo()
            .find_by_email(&email)
            .await
            .is_ok()
        {
            return Err(DomainError::Conflict {
                message: "Email already exists".to_string(),
            });
        }

        // Hash password
        let password_hash = self.auth_service.hash_password(&req.password)?;

        self.admin_repo
            .create_user(&username, &email, &password_hash, req.role)
            .await
    }

    pub async fn count_admins(&self) -> DomainResult<i64> {
        self.auth_service.user_repo().count_admins().await
    }

    async fn ensure_role_guardrails_on_update(
        &self,
        current_user: &User,
        next_role: Option<Role>,
        next_disabled: Option<bool>,
    ) -> DomainResult<()> {
        let admin_count = self.auth_service.user_repo().count_admins().await?;

        if current_user.role.is_admin() && admin_count <= 1 {
            if next_role.is_some_and(|role| !role.is_admin()) {
                return Err(DomainError::Conflict {
                    message: "Cannot change the last admin user role".to_string(),
                });
            }

            if next_disabled == Some(true) {
                return Err(DomainError::Conflict {
                    message: "Cannot disable the last admin user".to_string(),
                });
            }
        }

        Ok(())
    }

    pub async fn update_user(&self, user_id: &UserId, req: UpdateUserRequest) -> DomainResult<()> {
        let UpdateUserRequest {
            role,
            disabled,
            email,
            username,
        } = req;

        // Check if user exists
        let current_user = self.auth_service.user_repo().find_by_id(user_id).await?;

        self.ensure_role_guardrails_on_update(&current_user, role, disabled)
            .await?;

        // 改名：校验字符集，避免再次写入读不出来的账号
        if let Some(raw_username) = normalize_message(username.as_deref()) {
            let new_username = Username::new(&raw_username)?;
            if current_user.username.as_str() != new_username.as_str() {
                self.admin_repo
                    .update_user_username(user_id, &new_username)
                    .await?;
            }
        }

        if let Some(role) = role {
            self.admin_repo.update_user_role(user_id, role).await?;
        }

        if let Some(disabled) = disabled {
            if disabled {
                self.admin_repo.disable_user(user_id).await?;
            } else {
                self.admin_repo.enable_user(user_id).await?;
            }
        }

        if let Some(email) = normalize_message(email.as_deref()) {
            let email = EmailAddress::new(&email)?;
            if current_user.email.as_ref() != Some(&email) {
                if let Ok(existing_user) = self.auth_service.user_repo().find_by_email(&email).await
                    && existing_user.id != *user_id
                {
                    return Err(DomainError::Conflict {
                        message: "Email already exists".to_string(),
                    });
                }
                self.auth_service
                    .user_repo()
                    .update_email(user_id, &email)
                    .await?;
            }
        }

        Ok(())
    }

    /// 重置密码：使用与登录一致的哈希方案，并让该用户的所有会话失效。
    ///
    /// 忘记密码时的恢复路径（离线操作，不依赖邮件/SMTP）。
    pub async fn reset_password(&self, user_id: &UserId, password: &str) -> DomainResult<()> {
        if password.chars().count() < 8 {
            return Err(DomainError::Validation {
                message: "Password must be at least 8 characters".to_string(),
            });
        }

        // 先确认用户存在（不存在的 id 直接 NotFound）
        self.auth_service.user_repo().find_by_id(user_id).await?;

        let hash = AuthService::hash_password_for_storage(password)?;
        self.auth_service
            .user_repo()
            .update_password(user_id, &hash)
            .await?;

        Ok(())
    }

    /// 设置已经哈希好的密码（供自动化脚本使用）。
    pub async fn set_password_hash(
        &self,
        user_id: &UserId,
        password_hash: &str,
    ) -> DomainResult<()> {
        if password_hash.trim().is_empty() {
            return Err(DomainError::Validation {
                message: "Password hash must not be empty".to_string(),
            });
        }

        self.auth_service.user_repo().find_by_id(user_id).await?;
        self.auth_service
            .user_repo()
            .update_password(user_id, password_hash)
            .await
    }

    pub async fn revoke_user_sessions(&self, user_id: &UserId) -> DomainResult<()> {
        self.auth_service.user_repo().find_by_id(user_id).await?;
        self.auth_service
            .session_repo()
            .revoke_user_sessions(user_id)
            .await
    }

    pub async fn delete_user(&self, user_id: &UserId) -> DomainResult<()> {
        // Check if user exists
        let user = self.auth_service.user_repo().find_by_id(user_id).await?;
        let admin_count = self.auth_service.user_repo().count_admins().await?;

        if user.role.is_admin() && admin_count <= 1 {
            return Err(DomainError::Conflict {
                message: "Cannot delete the last admin user".to_string(),
            });
        }

        self.admin_repo.delete_user(user_id).await
    }

    pub async fn reset_user_password(
        &self,
        user_id: &UserId,
        new_password: &str,
    ) -> DomainResult<()> {
        // Check if user exists
        self.auth_service.user_repo().find_by_id(user_id).await?;

        let password_hash = self.auth_service.hash_password(new_password)?;
        self.admin_repo
            .reset_user_password(user_id, &password_hash)
            .await
    }

    pub async fn get_user_details(&self, user_id: &UserId) -> DomainResult<AdminUserSummary> {
        let user = self.auth_service.user_repo().find_by_id(user_id).await?;

        Ok(AdminUserSummary {
            id: user.id,
            username: user.username.as_str().to_string(),
            email: user.email.as_ref().map(|email| email.as_str().to_string()),
            role: user.role,
            disabled: user.disabled,
            created_at: user.created_at,
            last_login: None, // TODO: implement last login tracking
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use camino::Utf8PathBuf;
    use tempfile::TempDir;
    use vfiles_domain::{NamespaceRepo, SnapshotRepo, UserRepo};
    use vfiles_infra_sqlite::{
        FsBlobStore, FsUploadStore, SqliteEntryRepo, SqliteMigrations, SqliteNamespaceRepo,
        SqlitePoolFactory, SqliteSnapshotRepo, SqliteUserRepo,
    };

    struct CorruptUploadMetadataReader {
        path: std::path::PathBuf,
        bytes: &'static [u8],
        emitted: bool,
    }

    impl tokio::io::AsyncRead for CorruptUploadMetadataReader {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            if !self.emitted {
                std::fs::write(&self.path, b"corrupted after session lookup")?;
                buf.put_slice(self.bytes);
                self.emitted = true;
            }
            std::task::Poll::Ready(Ok(()))
        }
    }

    struct TestContext {
        _temp_dir: TempDir,
        storage_root: Utf8PathBuf,
        namespace_id: NamespaceId,
        user_id: UserId,
        entry_repo: SqliteEntryRepo,
        blob_store: FsBlobStore,
        snapshot_repo: SqliteSnapshotRepo,
        workspace_service:
            WorkspaceService<SqliteEntryRepo, SqliteSnapshotRepo, FsBlobStore, FsUploadStore>,
        upload_service:
            UploadService<SqliteEntryRepo, SqliteSnapshotRepo, FsBlobStore, FsUploadStore>,
        history_service:
            HistoryService<SqliteEntryRepo, SqliteSnapshotRepo, FsBlobStore, SqliteUserRepo>,
    }

    impl TestContext {
        async fn new() -> Self {
            let temp_dir = tempfile::tempdir().expect("tempdir should be created");
            let storage_root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
                .expect("tempdir path should be valid utf-8");
            let database_path = storage_root.join("vfiles.db");

            let pool = SqlitePoolFactory::connect(database_path.as_path())
                .await
                .expect("sqlite pool should connect");
            SqliteMigrations::run(&pool)
                .await
                .expect("migrations should succeed");

            let user_repo = SqliteUserRepo::new(pool.clone());
            let user_id = user_repo
                .create_admin("admin", "admin@example.com", "hash")
                .await
                .expect("admin should be created");

            let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
            let namespace_id = namespace_repo
                .create_default(&user_id, "default")
                .await
                .expect("default namespace should be created");
            let snapshot_repo = SqliteSnapshotRepo::new(pool.clone());
            let blob_store = FsBlobStore::new(pool.clone(), storage_root.join("blobs"));
            let upload_store = FsUploadStore::new(storage_root.join("uploads"));
            let entry_repo = SqliteEntryRepo::new(pool.clone());

            let workspace_service = WorkspaceService::new(
                entry_repo.clone(),
                snapshot_repo.clone(),
                blob_store.clone(),
                upload_store.clone(),
            );
            let upload_service = UploadService::new(
                entry_repo.clone(),
                snapshot_repo.clone(),
                blob_store.clone(),
                upload_store.clone(),
            );
            let history_service = HistoryService::new(
                entry_repo.clone(),
                snapshot_repo.clone(),
                blob_store.clone(),
                user_repo.clone(),
            );

            Self {
                _temp_dir: temp_dir,
                storage_root,
                namespace_id,
                user_id,
                entry_repo,
                blob_store,
                snapshot_repo,
                workspace_service,
                upload_service,
                history_service,
            }
        }

        fn path(value: &str) -> NormalizedPath {
            NormalizedPath::new(value).expect("path should be valid")
        }

        async fn upload_file(
            &self,
            directory: &NormalizedPath,
            filename: &str,
            bytes: &[u8],
            message: &str,
        ) -> UploadCompleteResponse {
            let upload = self
                .upload_service
                .init_upload(
                    &self.namespace_id,
                    directory,
                    filename,
                    bytes.len() as u64,
                    None,
                    None,
                    &self.user_id,
                )
                .await
                .expect("upload should initialize");

            self.upload_service
                .upload_part(&upload.upload_id, 0, bytes)
                .await
                .expect("upload part should succeed");

            self.upload_service
                .complete_upload(&upload.upload_id, None, Some(message))
                .await
                .expect("upload should complete")
        }

        async fn seed_file_without_snapshot(
            &self,
            directory: &NormalizedPath,
            filename: &str,
            bytes: &[u8],
            message: &str,
        ) -> (NormalizedPath, BlobId, ContentHash) {
            let file_path = uploaded_file_path(directory, filename).expect("path should be valid");
            let entry_id = self
                .entry_repo
                .create_entry(
                    &self.namespace_id,
                    &file_path,
                    EntryKind::File,
                    &self.user_id,
                )
                .await
                .expect("entry should be created");
            let mime_type = guess_mime_type(filename);
            let (blob_id, content_hash, _) = self
                .blob_store
                .store_blob(bytes, None)
                .await
                .expect("blob should be stored");

            self.entry_repo
                .create_version(
                    &entry_id,
                    Some(&blob_id),
                    Some(&content_hash),
                    bytes.len() as u64,
                    mime_type.as_deref(),
                    &self.user_id,
                    Some(message),
                )
                .await
                .expect("version should be created");

            (file_path, blob_id, content_hash)
        }

        fn blob_file_count(&self) -> usize {
            fn count_files(path: &std::path::Path) -> usize {
                let Ok(entries) = std::fs::read_dir(path) else {
                    return 0;
                };

                entries
                    .filter_map(Result::ok)
                    .map(|entry| {
                        let Ok(file_type) = entry.file_type() else {
                            return 0;
                        };
                        if file_type.is_dir() {
                            count_files(&entry.path())
                        } else {
                            1
                        }
                    })
                    .sum()
            }

            count_files(self.storage_root.join("blobs").as_std_path())
        }
    }

    #[tokio::test]
    async fn services_support_upload_snapshot_tree_history_and_restore() {
        let context = TestContext::new().await;
        let docs = TestContext::path("docs");
        let file_path = TestContext::path("docs/note.txt");

        context
            .workspace_service
            .create_directory(
                &context.namespace_id,
                &docs,
                Some("create docs"),
                &context.user_id,
            )
            .await
            .expect("directory should be created");

        let first = context
            .upload_file(
                &docs,
                "note.txt",
                b"hello from version one\n",
                "first version",
            )
            .await;
        let second = context
            .upload_file(
                &docs,
                "note.txt",
                b"hello from version two\n",
                "second version",
            )
            .await;

        let snapshot_tree = context
            .workspace_service
            .tree(
                &context.namespace_id,
                &docs,
                Some(
                    &first
                        .mutation
                        .as_ref()
                        .expect("upload snapshot should be created")
                        .snapshot_id,
                ),
            )
            .await
            .expect("snapshot tree should load");
        assert_eq!(snapshot_tree.items.len(), 1);
        assert_eq!(snapshot_tree.items[0].version_id, Some(first.version.id));

        let live_tree = context
            .workspace_service
            .tree(&context.namespace_id, &docs, None)
            .await
            .expect("live tree should load");
        assert_eq!(live_tree.summary.total_files, 1);
        assert_eq!(live_tree.items[0].version_id, Some(second.version.id));

        let live_content = context
            .workspace_service
            .read_file_bytes(&context.namespace_id, &file_path, None)
            .await
            .expect("live file content should load");
        assert_eq!(live_content.bytes, b"hello from version two\n");

        let snapshot_commit = first
            .mutation
            .as_ref()
            .expect("upload snapshot should be created")
            .snapshot_id
            .to_string();
        let snapshot_content = context
            .workspace_service
            .read_file_bytes(
                &context.namespace_id,
                &file_path,
                Some(snapshot_commit.as_str()),
            )
            .await
            .expect("snapshot file content should load");
        assert_eq!(snapshot_content.bytes, b"hello from version one\n");

        let live_archive = context
            .workspace_service
            .download_directory_archive(&context.namespace_id, &docs, None)
            .await
            .expect("live directory archive should build");
        let mut live_archive_bytes = Vec::new();
        let mut live_archive_reader = live_archive.reader;
        live_archive_reader
            .read_to_end(&mut live_archive_bytes)
            .await
            .expect("live archive stream should read");
        assert_eq!(live_archive_bytes.len() as u64, live_archive.size_bytes);
        let mut live_zip = zip::ZipArchive::new(std::io::Cursor::new(live_archive_bytes))
            .expect("live archive should open");
        let mut live_file = live_zip
            .by_name("docs/note.txt")
            .expect("live archive should contain file");
        let mut live_extracted = Vec::new();
        std::io::Read::read_to_end(&mut live_file, &mut live_extracted)
            .expect("live archive entry should read");
        assert_eq!(live_extracted, b"hello from version two\n");

        let first_version_commit = first.version.id.to_string();
        let first_version_archive = context
            .workspace_service
            .download_directory_archive(
                &context.namespace_id,
                &docs,
                Some(first_version_commit.as_str()),
            )
            .await
            .expect("versioned directory archive should build");
        let mut first_version_archive_bytes = Vec::new();
        let mut first_version_archive_reader = first_version_archive.reader;
        first_version_archive_reader
            .read_to_end(&mut first_version_archive_bytes)
            .await
            .expect("versioned archive stream should read");
        let mut first_version_zip =
            zip::ZipArchive::new(std::io::Cursor::new(first_version_archive_bytes))
                .expect("versioned archive should open");
        let mut first_version_file = first_version_zip
            .by_name("docs/note.txt")
            .expect("versioned archive should contain file");
        let mut first_version_extracted = Vec::new();
        std::io::Read::read_to_end(&mut first_version_file, &mut first_version_extracted)
            .expect("versioned archive entry should read");
        assert_eq!(first_version_extracted, b"hello from version one\n");

        let snapshot_archive = context
            .workspace_service
            .download_directory_archive(
                &context.namespace_id,
                &docs,
                Some(
                    first
                        .mutation
                        .as_ref()
                        .expect("upload snapshot should be created")
                        .snapshot_id
                        .to_string()
                        .as_str(),
                ),
            )
            .await
            .expect("snapshot directory archive should build");
        let mut snapshot_archive_bytes = Vec::new();
        let mut snapshot_archive_reader = snapshot_archive.reader;
        snapshot_archive_reader
            .read_to_end(&mut snapshot_archive_bytes)
            .await
            .expect("snapshot archive stream should read");
        let mut snapshot_zip = zip::ZipArchive::new(std::io::Cursor::new(snapshot_archive_bytes))
            .expect("snapshot archive should open");
        let mut snapshot_file = snapshot_zip
            .by_name("docs/note.txt")
            .expect("snapshot archive should contain file");
        let mut snapshot_extracted = Vec::new();
        std::io::Read::read_to_end(&mut snapshot_file, &mut snapshot_extracted)
            .expect("snapshot archive entry should read");
        assert_eq!(snapshot_extracted, b"hello from version one\n");

        let history = context
            .history_service
            .entry_history(&context.namespace_id, &file_path, None, 10)
            .await
            .expect("history should load");
        assert_eq!(history.items.len(), 2);
        assert!(history.items[0].is_current);
        assert_eq!(history.items[0].message.as_deref(), Some("second version"));

        let diff = context
            .history_service
            .diff_entry(&context.namespace_id, &file_path, &second.version.id, None)
            .await
            .expect("diff should load");
        assert!(diff.contains("hello from version one"));
        assert!(diff.contains("hello from version two"));

        context
            .history_service
            .restore_version(
                &context.namespace_id,
                &file_path,
                &first.version.id,
                Some("rollback"),
                &context.user_id,
            )
            .await
            .expect("restore should succeed");

        let restored_history = context
            .history_service
            .entry_history(&context.namespace_id, &file_path, None, 10)
            .await
            .expect("history should reload after restore");
        assert_eq!(restored_history.items.len(), 3);
        assert!(restored_history.items[0].is_current);
        assert_eq!(
            restored_history.items[0].message.as_deref(),
            Some("rollback")
        );
    }

    #[tokio::test]
    async fn upload_service_reports_missing_parts_and_rejects_incomplete_completion() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let total_size = 2 * 1024 * 1024;

        let upload = context
            .upload_service
            .init_upload(
                &context.namespace_id,
                &root,
                "large.bin",
                total_size,
                None,
                Some(1024 * 1024),
                &context.user_id,
            )
            .await
            .expect("upload should initialize");
        assert_eq!(upload.total_parts, 2);

        let first_chunk = vec![b'a'; 1024 * 1024];
        context
            .upload_service
            .upload_part(&upload.upload_id, 0, &first_chunk)
            .await
            .expect("first chunk should upload");

        let status = context
            .upload_service
            .get_upload_status(&upload.upload_id)
            .await
            .expect("status should be readable");
        assert_eq!(status.received_parts, vec![0]);
        assert_eq!(status.missing_parts, vec![1]);

        let error = context
            .upload_service
            .complete_upload(&upload.upload_id, None, None)
            .await
            .expect_err("incomplete upload should fail");
        assert!(matches!(error, DomainError::UploadConflict));
    }

    #[tokio::test]
    async fn conditional_stream_upload_rejects_a_stale_snapshot_at_commit() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let initial = context
            .upload_file(&root, "conditional.txt", b"initial", "initial")
            .await;
        let path = TestContext::path("conditional.txt");
        let entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &path)
            .await
            .expect("entry lookup should succeed")
            .expect("entry should exist");
        let pending = context
            .upload_service
            .init_stream_upload_unknown_size(
                &context.namespace_id,
                &root,
                "conditional.txt",
                None,
                &context.user_id,
            )
            .await
            .expect("conditional upload should initialize");

        let newer = context
            .upload_file(&root, "conditional.txt", b"newer", "newer")
            .await;
        let condition = vfiles_domain::EntryWriteCondition {
            namespace_id: context.namespace_id,
            path: path.clone(),
            check_entry_state: true,
            expected_entry_id: Some(entry.id),
            expected_version_id: Some(initial.version.id),
            expected_lock_tokens: None,
            expected_additional_lock_states: None,
        };
        let result = context
            .upload_service
            .complete_upload_from_stream_unknown_size_with_condition(
                &pending.upload_id,
                Some("WebDAV PUT"),
                Box::new(std::io::Cursor::new(b"stale body".to_vec())),
                &condition,
            )
            .await;

        assert!(matches!(result, Err(DomainError::PreconditionFailed)));
        let versions = context
            .entry_repo
            .find_versions_for_entries(&[entry.id])
            .await
            .expect("versions should be readable");
        assert_eq!(versions.len(), 2);
        assert_eq!(
            versions
                .iter()
                .max_by_key(|version| version.version_no)
                .map(|version| version.id),
            Some(newer.version.id)
        );
        context
            .upload_service
            .cancel_upload(&pending.upload_id)
            .await
            .expect("rejected upload session should be cancellable");
    }

    #[tokio::test]
    async fn conditional_stream_upload_can_create_a_previously_absent_resource() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let path = TestContext::path("new-conditional.txt");
        let pending = context
            .upload_service
            .init_stream_upload_unknown_size(
                &context.namespace_id,
                &root,
                "new-conditional.txt",
                None,
                &context.user_id,
            )
            .await
            .expect("conditional upload should initialize");
        let condition = vfiles_domain::EntryWriteCondition {
            namespace_id: context.namespace_id,
            path: path.clone(),
            check_entry_state: true,
            expected_entry_id: None,
            expected_version_id: None,
            expected_lock_tokens: None,
            expected_additional_lock_states: None,
        };

        let completed = context
            .upload_service
            .complete_upload_from_stream_unknown_size_with_condition(
                &pending.upload_id,
                Some("WebDAV PUT"),
                Box::new(std::io::Cursor::new(b"created".to_vec())),
                &condition,
            )
            .await
            .expect("conditional create should succeed while the resource is absent");

        assert_eq!(completed.version.version_no, 1);
        let entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &path)
            .await
            .expect("entry lookup should succeed")
            .expect("new resource should be present");
        assert_eq!(entry.current_version_id, Some(completed.version.id));
    }

    #[tokio::test]
    async fn conditional_delete_rejects_stale_version_and_accepts_current_version() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let initial = context
            .upload_file(&root, "conditional-delete.txt", b"initial", "initial")
            .await;
        let path = TestContext::path("conditional-delete.txt");
        let entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &path)
            .await
            .expect("entry lookup should succeed")
            .expect("entry should exist");
        let stale_condition = vfiles_domain::EntryWriteCondition {
            namespace_id: context.namespace_id,
            path: path.clone(),
            check_entry_state: true,
            expected_entry_id: Some(entry.id),
            expected_version_id: Some(initial.version.id),
            expected_lock_tokens: None,
            expected_additional_lock_states: None,
        };
        let newer = context
            .upload_file(&root, "conditional-delete.txt", b"newer", "newer")
            .await;

        let stale_delete = context
            .workspace_service
            .delete_entries_with_condition(
                &context.namespace_id,
                std::slice::from_ref(&path),
                Some("stale WebDAV DELETE"),
                &context.user_id,
                &stale_condition,
            )
            .await;
        assert!(matches!(stale_delete, Err(DomainError::PreconditionFailed)));
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &path)
                .await
                .expect("entry lookup should succeed")
                .is_some()
        );

        let current_condition = vfiles_domain::EntryWriteCondition {
            expected_version_id: Some(newer.version.id),
            ..stale_condition
        };
        context
            .workspace_service
            .delete_entries_with_condition(
                &context.namespace_id,
                std::slice::from_ref(&path),
                Some("current WebDAV DELETE"),
                &context.user_id,
                &current_condition,
            )
            .await
            .expect("matching conditional delete should succeed");
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &path)
                .await
                .expect("entry lookup should succeed")
                .is_none()
        );
    }

    #[tokio::test]
    async fn conditional_move_rejects_stale_source_and_preserves_overwrite_target() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let source = context
            .upload_file(&root, "conditional-move.txt", b"initial", "initial")
            .await;
        context
            .upload_file(&root, "conditional-move-dest.txt", b"target", "target")
            .await;
        let source_path = TestContext::path("conditional-move.txt");
        let destination = TestContext::path("conditional-move-dest.txt");
        let entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &source_path)
            .await
            .expect("source lookup should succeed")
            .expect("source should exist");
        let stale_condition = vfiles_domain::EntryWriteCondition {
            namespace_id: context.namespace_id,
            path: source_path.clone(),
            check_entry_state: true,
            expected_entry_id: Some(entry.id),
            expected_version_id: Some(source.version.id),
            expected_lock_tokens: None,
            expected_additional_lock_states: None,
        };
        let newer = context
            .upload_file(&root, "conditional-move.txt", b"newer", "newer")
            .await;

        let no_overwrite_destination = TestContext::path("conditional-no-overwrite-dest.txt");
        let stale_move_without_overwrite = context
            .workspace_service
            .move_entry_overwriting_with_condition(
                &context.namespace_id,
                &source_path,
                &no_overwrite_destination,
                Some("stale WebDAV MOVE"),
                &context.user_id,
                MoveOptions {
                    condition: Some(&stale_condition),
                    ..MoveOptions::default()
                },
            )
            .await;
        assert!(matches!(
            stale_move_without_overwrite,
            Err(DomainError::PreconditionFailed)
        ));
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &no_overwrite_destination)
                .await
                .expect("destination lookup should succeed")
                .is_none()
        );

        let stale_move = context
            .workspace_service
            .move_entry_overwriting_with_condition(
                &context.namespace_id,
                &source_path,
                &destination,
                Some("stale WebDAV MOVE"),
                &context.user_id,
                MoveOptions {
                    overwrite_destination: true,
                    condition: Some(&stale_condition),
                    ..MoveOptions::default()
                },
            )
            .await;
        assert!(matches!(stale_move, Err(DomainError::PreconditionFailed)));
        assert_eq!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &source_path)
                .await
                .expect("source lookup should succeed")
                .and_then(|entry| entry.current_version_id),
            Some(newer.version.id)
        );
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &destination)
                .await
                .expect("destination lookup should succeed")
                .is_some(),
            "failed conditional MOVE must preserve the overwrite target"
        );

        let current_condition = vfiles_domain::EntryWriteCondition {
            expected_version_id: Some(newer.version.id),
            ..stale_condition
        };
        context
            .workspace_service
            .move_entry_overwriting_with_condition(
                &context.namespace_id,
                &source_path,
                &destination,
                Some("current WebDAV MOVE"),
                &context.user_id,
                MoveOptions {
                    overwrite_destination: true,
                    condition: Some(&current_condition),
                    ..MoveOptions::default()
                },
            )
            .await
            .expect("matching conditional MOVE should succeed");
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &source_path)
                .await
                .expect("source lookup should succeed")
                .is_none()
        );
        assert_eq!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &destination)
                .await
                .expect("destination lookup should succeed")
                .and_then(|entry| entry.current_version_id),
            Some(newer.version.id)
        );
    }

    #[tokio::test]
    async fn conditional_copy_uses_a_consistent_source_snapshot() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let initial = context
            .upload_file(&root, "conditional-copy.txt", b"initial", "initial")
            .await;
        context
            .upload_file(&root, "conditional-copy-dest.txt", b"target", "target")
            .await;
        let source_path = TestContext::path("conditional-copy.txt");
        let destination = TestContext::path("conditional-copy-dest.txt");
        let entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &source_path)
            .await
            .expect("source lookup should succeed")
            .expect("source should exist");
        let stale_condition = vfiles_domain::EntryWriteCondition {
            namespace_id: context.namespace_id,
            path: source_path.clone(),
            check_entry_state: true,
            expected_entry_id: Some(entry.id),
            expected_version_id: Some(initial.version.id),
            expected_lock_tokens: None,
            expected_additional_lock_states: None,
        };
        let newer = context
            .upload_file(&root, "conditional-copy.txt", b"newer", "newer")
            .await;

        let stale_copy = context
            .workspace_service
            .copy_entries_with_options(
                &context.namespace_id,
                &source_path,
                &destination,
                Some("stale WebDAV COPY"),
                &context.user_id,
                CopyOptions {
                    overwrite: true,
                    depth_infinity: true,
                    condition: Some(stale_condition.clone()),
                    destination_lock_states: Vec::new(),
                },
            )
            .await;
        assert!(matches!(stale_copy, Err(DomainError::PreconditionFailed)));
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &destination)
                .await
                .expect("destination lookup should succeed")
                .is_some(),
            "failed conditional COPY must preserve the overwrite target"
        );

        let current_condition = vfiles_domain::EntryWriteCondition {
            expected_version_id: Some(newer.version.id),
            ..stale_condition
        };
        context
            .workspace_service
            .copy_entries_with_options(
                &context.namespace_id,
                &source_path,
                &destination,
                Some("current WebDAV COPY"),
                &context.user_id,
                CopyOptions {
                    overwrite: true,
                    depth_infinity: true,
                    condition: Some(current_condition),
                    destination_lock_states: Vec::new(),
                },
            )
            .await
            .expect("matching conditional COPY should succeed");
        let copied = context
            .entry_repo
            .find_by_path(&context.namespace_id, &destination)
            .await
            .expect("destination lookup should succeed")
            .expect("copy destination should exist");
        let copied_version = context
            .entry_repo
            .find_version(
                &copied
                    .current_version_id
                    .expect("copied version should exist"),
            )
            .await
            .expect("copied version lookup should succeed");
        assert_eq!(copied_version.blob_id, newer.version.blob_id);
    }

    #[tokio::test]
    async fn failed_stream_commit_removes_new_blob_when_target_becomes_a_directory() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let target = TestContext::path("race.txt");
        let bytes = b"blob must be rolled back".to_vec();
        let upload = context
            .upload_service
            .init_upload(
                &context.namespace_id,
                &root,
                "race.txt",
                bytes.len() as u64,
                None,
                None,
                &context.user_id,
            )
            .await
            .expect("upload should initialize before the path race");
        context
            .entry_repo
            .create_entry(
                &context.namespace_id,
                &target,
                EntryKind::Directory,
                &context.user_id,
            )
            .await
            .expect("racing directory should be created");

        let error = context
            .upload_service
            .complete_upload_from_stream(
                &upload.upload_id,
                None,
                None,
                Box::new(std::io::Cursor::new(bytes)),
            )
            .await
            .expect_err("the concurrent directory must reject the file upload");
        assert!(matches!(error, DomainError::PathConflict { .. }));
        assert!(
            context
                .blob_store
                .list_stored_blob_files()
                .await
                .expect("blob listing should succeed")
                .is_empty()
        );
        assert_eq!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &target)
                .await
                .expect("target lookup should succeed")
                .expect("racing directory should remain")
                .entry_type,
            EntryKind::Directory
        );
    }

    #[tokio::test]
    async fn committed_upload_succeeds_when_session_cleanup_metadata_is_unreadable() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let bytes = b"object committed before session cleanup";
        let upload = context
            .upload_service
            .init_upload(
                &context.namespace_id,
                &root,
                "committed.txt",
                bytes.len() as u64,
                None,
                None,
                &context.user_id,
            )
            .await
            .expect("upload should initialize");
        let metadata_path = context
            .storage_root
            .join("uploads")
            .join(upload.upload_id.to_string())
            .join("metadata.json")
            .into_std_path_buf();

        let completed = context
            .upload_service
            .complete_upload_from_stream(
                &upload.upload_id,
                None,
                None,
                Box::new(CorruptUploadMetadataReader {
                    path: metadata_path,
                    bytes,
                    emitted: false,
                }),
            )
            .await
            .expect("session cleanup failure must not turn a committed object into a failed write");

        assert_eq!(completed.upload.state, UploadState::Completed);
        assert_eq!(
            context
                .blob_store
                .get_blob(completed.version.blob_id.as_ref().unwrap())
                .await
                .expect("blob lookup should succeed")
                .expect("committed blob should exist"),
            bytes
        );
        assert!(
            !context
                .storage_root
                .join("uploads")
                .join(upload.upload_id.to_string())
                .exists(),
            "upload session storage should be removed best-effort"
        );
    }

    #[tokio::test]
    async fn committed_upload_succeeds_when_snapshot_creation_fails() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let bytes = b"committed object with failed snapshot".to_vec();
        let upload = context
            .upload_service
            .init_upload(
                &context.namespace_id,
                &root,
                "snapshot-failure.txt",
                bytes.len() as u64,
                None,
                None,
                &context.user_id,
            )
            .await
            .expect("upload should initialize");
        let fault_pool =
            SqlitePoolFactory::connect(context.storage_root.join("vfiles.db").as_path())
                .await
                .expect("database should reopen for fault injection");
        sqlx::query(
            "CREATE TRIGGER reject_snapshot BEFORE INSERT ON snapshots BEGIN SELECT RAISE(ABORT, 'injected snapshot failure'); END",
        )
        .execute(&fault_pool)
        .await
        .expect("snapshot fault trigger should be installed");

        let completed = context
            .upload_service
            .complete_upload_from_stream(
                &upload.upload_id,
                None,
                None,
                Box::new(std::io::Cursor::new(bytes.clone())),
            )
            .await
            .expect("snapshot failure must not turn a committed object into a failed write");

        assert!(completed.mutation.is_none());
        assert_eq!(completed.version.size_bytes.as_u64(), bytes.len() as u64);
        assert_eq!(
            context
                .blob_store
                .get_blob(completed.version.blob_id.as_ref().unwrap())
                .await
                .expect("blob lookup should succeed")
                .expect("committed blob should exist"),
            bytes
        );
        fault_pool.close().await;
    }

    #[tokio::test]
    async fn version_and_s3_properties_roll_back_in_one_transaction() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let path = TestContext::path("atomic-properties.txt");
        let bytes = b"rollback version with property".to_vec();
        let upload = context
            .upload_service
            .init_upload(
                &context.namespace_id,
                &root,
                "atomic-properties.txt",
                bytes.len() as u64,
                None,
                None,
                &context.user_id,
            )
            .await
            .expect("upload should initialize");
        let fault_pool =
            SqlitePoolFactory::connect(context.storage_root.join("vfiles.db").as_path())
                .await
                .expect("database should reopen for fault injection");
        sqlx::query(
            "CREATE TRIGGER reject_object_property BEFORE INSERT ON entry_properties BEGIN SELECT RAISE(ABORT, 'injected property failure'); END",
        )
        .execute(&fault_pool)
        .await
        .expect("property fault trigger should be installed");
        let properties = |version_id: VersionId| {
            vec![EntryPropertyChange::Set {
                name: format!("s3-etag:{version_id}"),
                value: "0123456789abcdef0123456789abcdef".to_string(),
            }]
        };

        let error = context
            .upload_service
            .complete_upload_from_stream_with_properties(
                &upload.upload_id,
                None,
                None,
                None,
                None,
                None,
                None,
                Some("S3 PUT"),
                Box::new(std::io::Cursor::new(bytes)),
                true,
                &properties,
            )
            .await
            .expect_err("property failure must abort the version transaction");

        assert!(matches!(error, DomainError::Internal { .. }));
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &path)
                .await
                .expect("entry lookup should succeed")
                .is_none()
        );
        assert!(
            context
                .blob_store
                .list_stored_blob_files()
                .await
                .expect("blob listing should succeed")
                .is_empty()
        );
        fault_pool.close().await;
    }

    #[tokio::test]
    async fn successful_upload_cleans_up_temporary_upload_directory() {
        let context = TestContext::new().await;
        let root = TestContext::path("");
        let bytes = b"temporary upload data\n";

        let upload = context
            .upload_service
            .init_upload(
                &context.namespace_id,
                &root,
                "temp.txt",
                bytes.len() as u64,
                None,
                None,
                &context.user_id,
            )
            .await
            .expect("upload should initialize");

        let upload_dir = context
            .storage_root
            .join("uploads")
            .join(upload.upload_id.to_string());

        context
            .upload_service
            .upload_part(&upload.upload_id, 0, bytes)
            .await
            .expect("upload part should succeed");

        assert!(upload_dir.as_std_path().exists());

        context
            .upload_service
            .complete_upload(&upload.upload_id, None, Some("cleanup temp upload"))
            .await
            .expect("upload should complete");

        assert!(!upload_dir.as_std_path().exists());
        assert!(
            !context
                .storage_root
                .join("uploads")
                .join("uploads")
                .exists()
        );
        assert!(!context.storage_root.join("blobs").join("blobs").exists());
    }

    #[tokio::test]
    async fn identical_uploads_reuse_one_blob_on_disk() {
        let context = TestContext::new().await;
        let docs = TestContext::path("docs");
        let file_path = TestContext::path("docs/note.txt");

        context
            .workspace_service
            .create_directory(
                &context.namespace_id,
                &docs,
                Some("create docs"),
                &context.user_id,
            )
            .await
            .expect("directory should be created");

        context
            .upload_file(&docs, "note.txt", b"same content\n", "first")
            .await;
        context
            .upload_file(&docs, "note.txt", b"same content\n", "second")
            .await;

        let entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &file_path)
            .await
            .expect("entry lookup should succeed")
            .expect("entry should exist");
        let history = context
            .entry_repo
            .get_entry_history(&entry.id, 10, None)
            .await
            .expect("history should load");

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content_hash, history[1].content_hash);
        assert_eq!(history[0].blob_id, history[1].blob_id);
        assert_eq!(context.blob_file_count(), 1);
    }

    #[tokio::test]
    async fn blob_metadata_returns_real_hash_storage_key_and_ref_count() {
        let context = TestContext::new().await;
        let docs = TestContext::path("docs");

        context
            .workspace_service
            .create_directory(
                &context.namespace_id,
                &docs,
                Some("create docs"),
                &context.user_id,
            )
            .await
            .expect("directory should be created");

        let upload = context
            .upload_file(&docs, "note.txt", b"metadata check\n", "first")
            .await;

        let blob_id = upload.version.blob_id.expect("upload should create blob");
        let metadata = context
            .blob_store
            .get_blob_metadata(&blob_id)
            .await
            .expect("blob metadata should load")
            .expect("blob metadata should exist");
        let blob_id_str = blob_id.to_string();
        let expected_storage_key = format!("blobs/{}/{}", &blob_id_str[0..2], &blob_id_str[2..],);

        assert_eq!(metadata.sha256_hex, upload.version.content_hash.as_str());
        assert_eq!(metadata.storage_key, expected_storage_key);
        assert_eq!(metadata.ref_count, 2);
    }

    #[tokio::test]
    async fn delete_reclaims_blob_when_no_snapshot_references_remain() {
        let context = TestContext::new().await;
        let docs = TestContext::path("docs");

        context
            .workspace_service
            .create_directory(
                &context.namespace_id,
                &docs,
                Some("create docs"),
                &context.user_id,
            )
            .await
            .expect("directory should be created");

        let (file_path, blob_id, _) = context
            .seed_file_without_snapshot(&docs, "ephemeral.txt", b"delete me\n", "seed")
            .await;
        assert_eq!(context.blob_file_count(), 1);

        let result = context
            .workspace_service
            .delete_entries(
                &context.namespace_id,
                std::slice::from_ref(&file_path),
                Some("delete file"),
                &context.user_id,
            )
            .await
            .expect("delete should succeed");

        assert!(result.warnings.is_empty());
        assert_eq!(context.blob_file_count(), 0);
        let metadata = context
            .blob_store
            .get_blob_metadata(&blob_id)
            .await
            .expect("blob metadata query should succeed");
        assert!(metadata.is_none());
    }

    #[tokio::test]
    async fn workspace_service_moves_and_deletes_entries() {
        let context = TestContext::new().await;
        let docs = TestContext::path("docs");
        let archive = TestContext::path("archive");

        context
            .workspace_service
            .create_directory(
                &context.namespace_id,
                &docs,
                Some("create docs"),
                &context.user_id,
            )
            .await
            .expect("docs should be created");
        context
            .workspace_service
            .create_directory(
                &context.namespace_id,
                &archive,
                Some("create archive"),
                &context.user_id,
            )
            .await
            .expect("archive should be created");

        context
            .upload_file(&docs, "note.txt", b"move me\n", "seed file")
            .await;

        let move_result = context
            .workspace_service
            .move_entries(
                &context.namespace_id,
                &[TestContext::path("docs/note.txt")],
                &archive,
                Some("move file"),
                &context.user_id,
                true, // Container（archive = 容器目录 ✗ 保持现状语义）
            )
            .await
            .expect("move should succeed");
        assert_eq!(move_result.changed_entries.len(), 1);
        assert_eq!(move_result.changed_entries[0].path, "archive/note.txt");

        let archive_tree = context
            .workspace_service
            .tree(&context.namespace_id, &archive, None)
            .await
            .expect("archive tree should load");
        assert_eq!(archive_tree.items.len(), 1);
        assert_eq!(archive_tree.items[0].path, "archive/note.txt");

        let delete_result = context
            .workspace_service
            .delete_entries(
                &context.namespace_id,
                &[TestContext::path("archive/note.txt")],
                Some("delete file"),
                &context.user_id,
            )
            .await
            .expect("delete should succeed");
        assert_eq!(delete_result.changed_entries.len(), 1);
        assert!(delete_result.warnings.is_empty());

        let snapshot_entries = context
            .snapshot_repo
            .get_snapshot_entries(&delete_result.snapshot_id)
            .await
            .expect("delete snapshot entries should load");
        assert!(snapshot_entries.iter().any(|entry| {
            entry.entry_path.as_str() == "archive/note.txt"
                && entry.change_type == ChangeType::Deleted
        }));

        let delete_snapshot_tree = context
            .workspace_service
            .tree(
                &context.namespace_id,
                &archive,
                Some(&delete_result.snapshot_id),
            )
            .await
            .expect("delete snapshot tree should load");
        assert!(delete_snapshot_tree.items.is_empty());

        let archive_tree_after_delete = context
            .workspace_service
            .tree(&context.namespace_id, &archive, None)
            .await
            .expect("archive tree should still load");
        assert!(archive_tree_after_delete.items.is_empty());
    }

    #[tokio::test]
    async fn workspace_service_overwrites_move_in_one_entry_transaction() {
        use tokio::io::AsyncReadExt;

        let context = TestContext::new().await;
        let root = TestContext::path("");
        context
            .upload_file(&root, "source.txt", b"source bytes", "source")
            .await;
        context
            .upload_file(&root, "target.txt", b"old target bytes", "target")
            .await;
        let source_meta = context
            .entry_repo
            .find_by_path_with_meta(&context.namespace_id, &TestContext::path("source.txt"))
            .await
            .expect("source metadata query should succeed")
            .expect("source should exist");
        assert_eq!(source_meta.size_bytes, Some(b"source bytes".len() as u64));
        assert_eq!(source_meta.mime_type.as_deref(), Some("text/plain"));

        let result = context
            .workspace_service
            .move_entry_overwriting(
                &context.namespace_id,
                &TestContext::path("source.txt"),
                &TestContext::path("target.txt"),
                Some("replace target with source"),
                &context.user_id,
                true,
            )
            .await
            .expect("overwrite move should succeed");
        assert_eq!(result.changed_entries.len(), 2);
        assert!(result.changed_entries.iter().any(|entry| {
            entry.path == "target.txt" && entry.change_type == ChangeType::Deleted
        }));
        assert!(result.changed_entries.iter().any(|entry| {
            entry.path == "target.txt" && entry.change_type == ChangeType::Renamed
        }));
        let snapshot_entries = context
            .snapshot_repo
            .get_snapshot_entries(&result.snapshot_id)
            .await
            .expect("overwrite snapshot should be readable");
        assert!(snapshot_entries.iter().any(|entry| {
            entry.entry_path.as_str() == "target.txt" && entry.change_type == ChangeType::Deleted
        }));
        assert!(snapshot_entries.iter().any(|entry| {
            entry.entry_path.as_str() == "target.txt" && entry.change_type == ChangeType::Renamed
        }));
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &TestContext::path("source.txt"))
                .await
                .expect("source lookup should succeed")
                .is_none()
        );
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &TestContext::path("target.txt"))
                .await
                .expect("destination lookup should succeed")
                .is_some()
        );

        let mut file = context
            .workspace_service
            .open_file(
                &context.namespace_id,
                &TestContext::path("target.txt"),
                None,
            )
            .await
            .expect("destination should open");
        let mut bytes = Vec::new();
        file.reader
            .read_to_end(&mut bytes)
            .await
            .expect("destination bytes should be readable");
        assert_eq!(bytes, b"source bytes");
    }

    #[tokio::test]
    async fn copy_depth_zero_copies_only_the_collection_root() {
        let context = TestContext::new().await;
        let source = TestContext::path("source");
        let source_child = TestContext::path("source/child.txt");
        let nested_directory = TestContext::path("source/nested");

        context
            .workspace_service
            .create_directory(
                &context.namespace_id,
                &source,
                Some("create copy source"),
                &context.user_id,
            )
            .await
            .expect("source collection should be created");
        context
            .workspace_service
            .create_directory(
                &context.namespace_id,
                &nested_directory,
                Some("create nested copy source"),
                &context.user_id,
            )
            .await
            .expect("nested source collection should be created");
        context
            .upload_file(&source, "child.txt", b"child", "create child")
            .await;
        context
            .upload_file(
                &nested_directory,
                "nested.txt",
                b"nested",
                "create nested child",
            )
            .await;
        let source_entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &source)
            .await
            .expect("source lookup should succeed")
            .expect("source collection should exist");
        context
            .entry_repo
            .set_entry_property(&source_entry.id, "urn:example\u{1f}color", "blue")
            .await
            .expect("source collection property should be saved");
        let source_child_entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &source_child)
            .await
            .expect("source child lookup should succeed")
            .expect("source child should exist");
        context
            .entry_repo
            .set_entry_property(&source_child_entry.id, "urn:example\u{1f}label", "child")
            .await
            .expect("source child property should be saved");

        context
            .workspace_service
            .copy_entries_with_options(
                &context.namespace_id,
                &source,
                &TestContext::path("shallow-copy"),
                Some("depth zero copy"),
                &context.user_id,
                CopyOptions {
                    overwrite: false,
                    depth_infinity: false,
                    condition: None,
                    destination_lock_states: Vec::new(),
                },
            )
            .await
            .expect("depth-zero copy should succeed");
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &TestContext::path("shallow-copy"))
                .await
                .expect("shallow root lookup should succeed")
                .is_some()
        );
        assert!(
            context
                .entry_repo
                .find_by_path(
                    &context.namespace_id,
                    &TestContext::path("shallow-copy/child.txt")
                )
                .await
                .expect("shallow child lookup should succeed")
                .is_none()
        );
        assert!(
            context
                .entry_repo
                .find_by_path(
                    &context.namespace_id,
                    &TestContext::path("shallow-copy/nested")
                )
                .await
                .expect("shallow nested collection lookup should succeed")
                .is_none()
        );
        let shallow_entry = context
            .entry_repo
            .find_by_path(&context.namespace_id, &TestContext::path("shallow-copy"))
            .await
            .expect("shallow root lookup should succeed")
            .expect("shallow root should exist");
        let shallow_properties = context
            .entry_repo
            .list_entry_properties(std::slice::from_ref(&shallow_entry.id))
            .await
            .expect("shallow properties should load");
        assert!(
            shallow_properties[&shallow_entry.id]
                .iter()
                .any(|(name, value)| name == "urn:example\u{1f}color" && value == "blue")
        );

        context
            .workspace_service
            .copy_entries_with_options(
                &context.namespace_id,
                &source,
                &TestContext::path("recursive-copy"),
                Some("infinite copy"),
                &context.user_id,
                CopyOptions {
                    overwrite: false,
                    depth_infinity: true,
                    condition: None,
                    destination_lock_states: Vec::new(),
                },
            )
            .await
            .expect("infinity copy should succeed");
        assert!(
            context
                .entry_repo
                .find_by_path(
                    &context.namespace_id,
                    &TestContext::path("recursive-copy/child.txt")
                )
                .await
                .expect("recursive child lookup should succeed")
                .is_some()
        );
        assert!(
            context
                .entry_repo
                .find_by_path(
                    &context.namespace_id,
                    &TestContext::path("recursive-copy/nested/nested.txt")
                )
                .await
                .expect("nested copied child lookup should succeed")
                .is_some()
        );
        let copied_child = context
            .entry_repo
            .find_by_path(
                &context.namespace_id,
                &TestContext::path("recursive-copy/child.txt"),
            )
            .await
            .expect("copied child lookup should succeed")
            .expect("copied child should exist");
        let copied_child_properties = context
            .entry_repo
            .list_entry_properties(std::slice::from_ref(&copied_child.id))
            .await
            .expect("copied child properties should load");
        assert!(
            copied_child_properties[&copied_child.id]
                .iter()
                .any(|(name, value)| name == "urn:example\u{1f}label" && value == "child")
        );
        assert!(
            context
                .entry_repo
                .find_by_path(&context.namespace_id, &source_child)
                .await
                .expect("source child lookup should succeed")
                .is_some()
        );
    }
}

#[cfg(test)]
mod maintenance_tests {
    use super::*;
    use camino::Utf8PathBuf;
    use vfiles_infra_sqlite::{
        FsBlobStore, SqliteEntryRepo, SqliteMigrations, SqliteNamespaceRepo, SqlitePoolFactory,
    };

    fn sample_blob(ref_count: u32, created_at: time::OffsetDateTime) -> Blob {
        Blob {
            id: BlobId::new(),
            sha256_hex: "hash".to_string(),
            storage_key: "key".to_string(),
            size_bytes: ByteSize::new(10),
            mime_type_detected: None,
            ref_count,
            created_at,
            verified_at: None,
        }
    }

    #[test]
    fn is_purgeable_requires_no_reference_and_grace_period() {
        let now = time::OffsetDateTime::now_utc();
        let cutoff = now - time::Duration::hours(1);

        let old = sample_blob(1, now - time::Duration::hours(2));
        let fresh = sample_blob(1, now);

        assert!(is_purgeable(&old, false, cutoff));
        assert!(!is_purgeable(&old, true, cutoff));
        assert!(!is_purgeable(&fresh, false, cutoff));
    }

    #[tokio::test]
    async fn purges_orphan_files_but_keeps_referenced_and_fresh_ones() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .expect("temp path should be utf-8");
        let pool = SqlitePoolFactory::connect(root.join("vfiles.db").as_path())
            .await
            .expect("pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());
        let blob_store = FsBlobStore::new(pool.clone(), root.join("blobs"));

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed")
            .await
            .expect("admin should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("namespace should be created");

        // 被引用的 blob（版本已建立）
        let (referenced_id, content_hash, _) = blob_store
            .store_blob(b"referenced", None)
            .await
            .expect("blob should store");
        let entry_id = entry_repo
            .create_entry(
                &namespace_id,
                &NormalizedPath::new("a.txt").expect("path should parse"),
                EntryKind::File,
                &user_id,
            )
            .await
            .expect("entry should be created");
        entry_repo
            .create_version(
                &entry_id,
                Some(&referenced_id),
                Some(&content_hash),
                10,
                Some("text/plain"),
                &user_id,
                Some("v1"),
            )
            .await
            .expect("version should be created");

        // 孤儿文件：流式上传已完成 blob 发布，但模拟进程在 SQLite 建立版本引用前崩溃。
        let (orphan_id, _, _, _) = blob_store
            .store_blob_stream(
                Box::new(tokio::io::BufReader::new(std::io::Cursor::new(
                    b"orphan-after-crash".to_vec(),
                ))),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("blob should store");
        let orphan_string = orphan_id.to_string();
        let orphan_path = root
            .join("blobs")
            .join(&orphan_string[0..2])
            .join(&orphan_string[2..]);
        backdate_file(
            &orphan_path,
            time::OffsetDateTime::now_utc() - time::Duration::hours(2),
        );

        // 刚写入的孤儿文件应受保护期保护
        let (fresh_id, _, _, _) = blob_store
            .store_blob_stream(
                Box::new(tokio::io::BufReader::new(std::io::Cursor::new(
                    b"fresh-orphan".to_vec(),
                ))),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("blob should store");

        // 模拟服务进程重启：清理器只依赖持久化 SQLite 与 blob 目录识别孤儿。
        pool.close().await;
        let reopened_pool = SqlitePoolFactory::connect(root.join("vfiles.db").as_path())
            .await
            .expect("database should reopen");
        SqliteMigrations::run(&reopened_pool)
            .await
            .expect("migrations should still succeed");
        let reopened_entry_repo = SqliteEntryRepo::new(reopened_pool.clone());
        let reopened_blob_store = FsBlobStore::new(reopened_pool.clone(), root.join("blobs"));
        let snapshot_repo = vfiles_infra_sqlite::SqliteSnapshotRepo::new(reopened_pool.clone());
        let service = MaintenanceService::new(
            reopened_blob_store.clone(),
            reopened_entry_repo,
            snapshot_repo,
        );
        assert!(
            reopened_blob_store
                .get_blob_metadata(&orphan_id)
                .await
                .expect("orphan metadata lookup should succeed")
                .is_none()
        );
        let report = service
            .purge_orphan_blobs(3600)
            .await
            .expect("purge should succeed");

        assert_eq!(report.removed, 1);
        assert!(report.freed_bytes > 0);
        assert!(!orphan_path.exists(), "orphan file should be removed");
        assert!(
            reopened_blob_store
                .get_blob_metadata(&referenced_id)
                .await
                .expect("lookup should succeed")
                .is_some()
        );

        let fresh_string = fresh_id.to_string();
        let fresh_path = root
            .join("blobs")
            .join(&fresh_string[0..2])
            .join(&fresh_string[2..]);
        assert!(fresh_path.exists(), "fresh file should be kept");

        reopened_pool.close().await;
    }

    /// 时间窗口：只裁剪「既不在最新 keep 个之内、又早于窗口」的快照。
    #[tokio::test]
    async fn prune_snapshots_respects_the_age_window() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .expect("temp path should be utf-8");
        let pool = SqlitePoolFactory::connect(root.join("vfiles.db").as_path())
            .await
            .expect("pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());
        let snapshot_repo = vfiles_infra_sqlite::SqliteSnapshotRepo::new(pool.clone());
        let blob_store = FsBlobStore::new(pool.clone(), root.join("blobs"));

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed")
            .await
            .expect("admin should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("namespace should be created");

        let mut ids = Vec::new();
        for index in 0..3 {
            ids.push(
                snapshot_repo
                    .create_snapshot(
                        &namespace_id,
                        Some(&format!("s{index}")),
                        SnapshotKind::AutoCommit,
                        &user_id,
                    )
                    .await
                    .expect("snapshot should be created"),
            );
        }

        // 明确时间顺序：最新的保持「刚刚创建」，另两条分别在 40/50 天前
        let stamps = [
            time::OffsetDateTime::now_utc() - time::Duration::days(50),
            time::OffsetDateTime::now_utc() - time::Duration::days(40),
            time::OffsetDateTime::now_utc(),
        ];
        for (snapshot_id, stamp) in ids.iter().zip(stamps) {
            let text = stamp
                .format(&time::format_description::well_known::Rfc3339)
                .expect("format");
            sqlx::query("UPDATE snapshots SET created_at = ? WHERE id = ?")
                .bind(&text)
                .bind(snapshot_id.to_string())
                .execute(&pool)
                .await
                .expect("backdate should succeed");
        }

        let service = MaintenanceService::new(blob_store, entry_repo, snapshot_repo.clone());

        // keep=10 时数量策略不会删任何东西（时间窗口是「与」条件）
        let report = service
            .prune_snapshots_with_age(10, Some(std::time::Duration::from_secs(30 * 86_400)))
            .await
            .expect("prune should succeed");
        assert_eq!(
            report.pruned_snapshots, 0,
            "没有超出数量上限时，时间窗口不应删除任何快照"
        );

        // keep=1 + 窗口 30 天 → 只删「超出最新 1 条」且「早于 30 天」的两条
        let report = service
            .prune_snapshots_with_age(1, Some(std::time::Duration::from_secs(30 * 86_400)))
            .await
            .expect("prune should succeed");
        assert_eq!(report.pruned_snapshots, 2);

        let remaining = snapshot_repo
            .list_all_snapshots()
            .await
            .expect("list should succeed");
        assert_eq!(remaining.len(), 1, "最新快照必须保留");
        assert_eq!(remaining[0].id, ids[2]);

        pool.close().await;
    }

    #[tokio::test]
    async fn prune_snapshots_keeps_newest_and_releases_references() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .expect("temp path should be utf-8");
        let pool = SqlitePoolFactory::connect(root.join("vfiles.db").as_path())
            .await
            .expect("pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());
        let snapshot_repo = vfiles_infra_sqlite::SqliteSnapshotRepo::new(pool.clone());
        let blob_store = FsBlobStore::new(pool.clone(), root.join("blobs"));

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed")
            .await
            .expect("admin should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("namespace should be created");

        let (blob_id, content_hash, _) = blob_store
            .store_blob(b"snapshot-data", None)
            .await
            .expect("blob should store");
        let path = NormalizedPath::new("a.txt").expect("path should parse");
        let entry_id = entry_repo
            .create_entry(&namespace_id, &path, EntryKind::File, &user_id)
            .await
            .expect("entry should be created");
        entry_repo
            .create_version(
                &entry_id,
                Some(&blob_id),
                Some(&content_hash),
                13,
                Some("text/plain"),
                &user_id,
                Some("v1"),
            )
            .await
            .expect("version should be created");

        for index in 0..3 {
            let snapshot_id = snapshot_repo
                .create_snapshot(
                    &namespace_id,
                    Some(&format!("s{index}")),
                    SnapshotKind::AutoCommit,
                    &user_id,
                )
                .await
                .expect("snapshot should be created");
            snapshot_repo
                .add_snapshot_entries(
                    &snapshot_id,
                    &[SnapshotEntry {
                        snapshot_id,
                        entry_id,
                        entry_path: path.clone(),
                        entry_kind: EntryKind::File,
                        entry_version_id: None,
                        blob_id: Some(blob_id),
                        size_bytes: Some(ByteSize::new(13)),
                        mime_type: Some("text/plain".to_string()),
                        version_no: Some(1),
                        change_type: ChangeType::Added,
                        created_by: Some(user_id),
                        created_at: Some(time::OffsetDateTime::now_utc()),
                    }],
                )
                .await
                .expect("snapshot entry should be added");
        }

        let refs_before = blob_store
            .get_blob_metadata(&blob_id)
            .await
            .expect("lookup should succeed")
            .expect("blob should exist")
            .ref_count;
        assert_eq!(refs_before, 4, "1 version + 3 snapshot references");

        let service =
            MaintenanceService::new(blob_store.clone(), entry_repo, snapshot_repo.clone());
        let report = service
            .prune_snapshots(1)
            .await
            .expect("prune should succeed");

        assert_eq!(report.pruned_snapshots, 2);
        assert_eq!(
            snapshot_repo
                .list_all_snapshots()
                .await
                .expect("snapshots should list")
                .len(),
            1
        );
        let refs_after = blob_store
            .get_blob_metadata(&blob_id)
            .await
            .expect("lookup should succeed")
            .expect("blob should exist")
            .ref_count;
        assert_eq!(refs_after, refs_before - 2);
    }

    #[tokio::test]
    async fn run_once_combines_snapshot_pruning_and_blob_purge() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .expect("temp path should be utf-8");
        let pool = SqlitePoolFactory::connect(root.join("vfiles.db").as_path())
            .await
            .expect("pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());
        let snapshot_repo = vfiles_infra_sqlite::SqliteSnapshotRepo::new(pool.clone());
        let blob_store = FsBlobStore::new(pool.clone(), root.join("blobs"));

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed")
            .await
            .expect("admin should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("namespace should be created");

        // 场景一：文件已被删除（版本引用随之释放），只剩旧快照还引用这个 blob
        let (blob_id, content_hash, _) = blob_store
            .store_blob(b"deleted-file", None)
            .await
            .expect("blob should store");
        let path = NormalizedPath::new("gone.txt").expect("path should parse");
        let entry_id = entry_repo
            .create_entry(&namespace_id, &path, EntryKind::File, &user_id)
            .await
            .expect("entry should be created");
        entry_repo
            .create_version(
                &entry_id,
                Some(&blob_id),
                Some(&content_hash),
                12,
                Some("text/plain"),
                &user_id,
                Some("v1"),
            )
            .await
            .expect("version should be created");

        let old_snapshot = snapshot_repo
            .create_snapshot(
                &namespace_id,
                Some("old"),
                SnapshotKind::AutoCommit,
                &user_id,
            )
            .await
            .expect("snapshot should be created");
        snapshot_repo
            .add_snapshot_entries(
                &old_snapshot,
                &[SnapshotEntry {
                    snapshot_id: old_snapshot,
                    entry_id,
                    entry_path: path.clone(),
                    entry_kind: EntryKind::File,
                    entry_version_id: None,
                    blob_id: Some(blob_id),
                    size_bytes: Some(ByteSize::new(12)),
                    mime_type: Some("text/plain".to_string()),
                    version_no: Some(1),
                    change_type: ChangeType::Added,
                    created_by: Some(user_id),
                    created_at: Some(time::OffsetDateTime::now_utc()),
                }],
            )
            .await
            .expect("snapshot entry should be added");

        // 模拟删除文件：条目连同版本引用一起释放，此时 blob 只剩快照引用
        entry_repo
            .delete_entries(&[entry_id])
            .await
            .expect("entry should be deleted");
        entry_repo
            .release_blob_references(&[(blob_id, 1)])
            .await
            .expect("version reference should be released");

        // 更新的空快照：裁剪时保留它
        snapshot_repo
            .create_snapshot(
                &namespace_id,
                Some("newest"),
                SnapshotKind::AutoCommit,
                &user_id,
            )
            .await
            .expect("snapshot should be created");

        // 场景二：中断上传留下的孤儿文件（有文件、无元数据行、已过保护期）
        let (orphan_id, _, _) = blob_store
            .store_blob(b"orphan", None)
            .await
            .expect("blob should store");
        let orphan_string = orphan_id.to_string();
        let orphan_path = root
            .join("blobs")
            .join(&orphan_string[0..2])
            .join(&orphan_string[2..]);
        backdate_file(
            &orphan_path,
            time::OffsetDateTime::now_utc() - time::Duration::hours(2),
        );

        let blob_string = blob_id.to_string();
        let released_path = root
            .join("blobs")
            .join(&blob_string[0..2])
            .join(&blob_string[2..]);
        assert!(released_path.exists());

        let service =
            MaintenanceService::new(blob_store.clone(), entry_repo, snapshot_repo.clone());
        let report = service
            .run_once(3600, 1, 0)
            .await
            .expect("maintenance run should succeed");

        // 旧快照被裁剪 → 其 blob 引用归零 → 行与文件一并清理
        assert_eq!(report.pruned_snapshots, 1);
        assert_eq!(report.released_blobs, 1);
        assert!(!released_path.exists(), "released blob file should be gone");
        // 同一轮里孤儿文件也被回收
        assert_eq!(report.purged_blobs, 1);
        assert!(!orphan_path.exists(), "orphan file should be gone");
        assert!(report.freed_bytes > 0);

        let snapshots = snapshot_repo
            .list_all_snapshots()
            .await
            .expect("snapshots should list");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            snapshots[0]
                .message
                .as_ref()
                .map(|message| message.as_str()),
            Some("newest")
        );

        pool.close().await;
    }

    #[tokio::test]
    async fn run_once_without_snapshot_retention_only_purges() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .expect("temp path should be utf-8");
        let pool = SqlitePoolFactory::connect(root.join("vfiles.db").as_path())
            .await
            .expect("pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());
        let snapshot_repo = vfiles_infra_sqlite::SqliteSnapshotRepo::new(pool.clone());
        let blob_store = FsBlobStore::new(pool.clone(), root.join("blobs"));

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed")
            .await
            .expect("admin should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("namespace should be created");

        let _snapshot_id = snapshot_repo
            .create_snapshot(
                &namespace_id,
                Some("keep-me"),
                SnapshotKind::AutoCommit,
                &user_id,
            )
            .await
            .expect("snapshot should be created");

        let service = MaintenanceService::new(blob_store, entry_repo, snapshot_repo.clone());
        let report = service
            .run_once(3600, 0, 0)
            .await
            .expect("maintenance run should succeed");

        assert_eq!(report.pruned_snapshots, 0);
        assert_eq!(report.released_blobs, 0);
        assert_eq!(report.purged_blobs, 0);
        assert_eq!(
            snapshot_repo
                .list_all_snapshots()
                .await
                .expect("snapshots should list")
                .len(),
            1,
            "keep=0 means snapshots are never pruned"
        );

        pool.close().await;
    }

    fn backdate_file(path: &Utf8PathBuf, when: time::OffsetDateTime) {
        let file = std::fs::File::options()
            .write(true)
            .open(path.as_std_path())
            .expect("blob file should open");
        file.set_modified(std::time::SystemTime::from(when))
            .expect("mtime should be set");
    }
}

#[cfg(test)]
mod cred_key_tests {
    use super::AuthService;

    #[test]
    fn key_is_stable_and_distinguishing() {
        // r9 凭据键守护：同凭据稳定 / 异凭据区分（原文零存 ✗ SHA-256）
        let a = AuthService::cred_key("admin", "pw1");
        let b = AuthService::cred_key("admin", "pw1");
        let c = AuthService::cred_key("admin", "pw2");
        let d = AuthService::cred_key("admim", "pw1");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
