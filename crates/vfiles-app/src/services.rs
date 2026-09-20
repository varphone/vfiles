use std::{collections::HashMap, io::Write};

use tokio::io::AsyncReadExt;
use vfiles_domain::*;
use vfiles_infra_sqlite::{SqliteSessionRepo, SqliteUserRepo};

fn normalize_message(message: Option<&str>) -> Option<String> {
    message.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn validate_filename(filename: &str) -> DomainResult<()> {
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

fn uploaded_file_path(
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

fn guess_mime_type(filename: &str) -> Option<String> {
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
struct PendingSnapshotEntry {
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

async fn collect_descendants<E>(
    entry_repo: &E,
    namespace_id: &NamespaceId,
    root: &Entry,
) -> DomainResult<Vec<Entry>>
where
    E: EntryRepo,
{
    // 单次范围查询取回 root 及其全部后代，避免按目录递归（O(目录数) 次查询）。
    entry_repo.find_subtree(namespace_id, &root.path_norm).await
}

async fn collect_namespace_entries<E>(
    entry_repo: &E,
    namespace_id: &NamespaceId,
) -> DomainResult<Vec<Entry>>
where
    E: EntryRepo,
{
    // 单次查询取回全部条目，避免按目录递归列举（O(目录数) 次查询）。
    let mut entries = entry_repo.find_all(namespace_id).await?;
    entries.sort_by(|left, right| left.path_norm.as_str().cmp(right.path_norm.as_str()));
    Ok(entries)
}

async fn resolve_current_version_for_entry<E>(
    entry_repo: &E,
    entry: &Entry,
) -> DomainResult<Option<EntryVersion>>
where
    E: EntryRepo,
{
    match (entry.entry_type, entry.current_version_id) {
        (EntryKind::File, Some(version_id)) => {
            Ok(Some(entry_repo.find_version(&version_id).await?))
        }
        _ => Ok(None),
    }
}

fn snapshot_change_type(entry_kind: EntryKind, version: Option<&EntryVersion>) -> ChangeType {
    version
        .map(|value| value.change_type)
        .unwrap_or(match entry_kind {
            EntryKind::Directory => ChangeType::Added,
            EntryKind::File => ChangeType::Added,
        })
}

async fn collect_snapshot_state<E>(
    entry_repo: &E,
    namespace_id: &NamespaceId,
    extra_entries: Vec<PendingSnapshotEntry>,
) -> DomainResult<Vec<PendingSnapshotEntry>>
where
    E: EntryRepo,
{
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

async fn ensure_directory_path<E>(
    entry_repo: &E,
    namespace_id: &NamespaceId,
    directory_path: &NormalizedPath,
    user_id: &UserId,
) -> DomainResult<Vec<ChangedEntry>>
where
    E: EntryRepo,
{
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

async fn create_snapshot_record<S>(
    snapshot_repo: &S,
    namespace_id: &NamespaceId,
    message: Option<&str>,
    kind: SnapshotKind,
    user_id: &UserId,
    snapshot_entries: Vec<PendingSnapshotEntry>,
) -> DomainResult<SnapshotId>
where
    S: SnapshotRepo,
{
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

async fn finalize_mutation<S>(
    snapshot_repo: &S,
    namespace_id: &NamespaceId,
    message: Option<&str>,
    user_id: &UserId,
    changed_entries: Vec<ChangedEntry>,
    snapshot_entries: Vec<PendingSnapshotEntry>,
    warnings: Vec<String>,
) -> DomainResult<MutationResult>
where
    S: SnapshotRepo,
{
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
    B: BlobStore,
    E: EntryRepo,
    S: SnapshotRepo,
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
    ) -> DomainResult<MaintenanceRunReport> {
        let prune = if snapshot_keep > 0 {
            self.prune_snapshots(snapshot_keep).await?
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
        let mut by_namespace: std::collections::HashMap<NamespaceId, Vec<Snapshot>> =
            std::collections::HashMap::new();
        for snapshot in self.snapshot_repo.list_all_snapshots().await? {
            by_namespace
                .entry(snapshot.namespace_id)
                .or_default()
                .push(snapshot);
        }

        let mut report = SnapshotPruneReport::default();
        for snapshots in by_namespace.values_mut() {
            // list_all_snapshots 已按创建时间倒序
            for snapshot in snapshots.iter().skip(keep as usize) {
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
}

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
        // Try to find user by username or email. Invalid identifier formats are
        // reported as invalid credentials so login failures do not reveal which
        // half of the identifier guessed incorrectly.
        let user = if req.username_or_email.contains('@') {
            let email = EmailAddress::new(&req.username_or_email)
                .map_err(|_| DomainError::InvalidCredentials)?;
            match self.user_repo.find_by_email(&email).await {
                Ok(user) => user,
                Err(DomainError::NotFound { .. }) => return Err(DomainError::InvalidCredentials),
                Err(err) => return Err(err),
            }
        } else {
            let username = Username::new(&req.username_or_email)
                .map_err(|_| DomainError::InvalidCredentials)?;
            match self.user_repo.find_by_username(&username).await {
                Ok(user) => user,
                Err(DomainError::NotFound { .. }) => return Err(DomainError::InvalidCredentials),
                Err(err) => return Err(err),
            }
        };

        if user.disabled {
            return Err(DomainError::InvalidCredentials);
        }

        if !self.verify_password(&req.password, &user.password_hash)? {
            return Err(DomainError::InvalidCredentials);
        }

        // Transparently migrate legacy SHA-256 password hashes after a
        // successful login. Failures must not block the login.
        if !user.password_hash.starts_with("$argon2")
            && let Ok(new_hash) = self.hash_password(&req.password)
        {
            let _ = self.user_repo.update_password(&user.id, &new_hash).await;
        }

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
}

pub struct FileContentStream {
    pub filename: String,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
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

#[derive(Debug, Clone)]
pub struct DirectoryArchive {
    pub filename: String,
    pub bytes: Vec<u8>,
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
    E: EntryRepo,
    S: SnapshotRepo,
    B: BlobStore,
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

    async fn version_for_entry(&self, entry: &Entry) -> DomainResult<Option<EntryVersion>> {
        resolve_current_version_for_entry(&self.entry_repo, entry).await
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

    async fn collect_live_directory_files(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> DomainResult<Vec<(NormalizedPath, BlobId)>> {
        let scope_entries = self.collect_scoped_entries(namespace_id, path).await?;

        let mut files = Vec::new();
        for entry in scope_entries {
            if entry.entry_type != EntryKind::File {
                continue;
            }
            let Some(version) = self.version_for_entry(&entry).await? else {
                continue;
            };
            let Some(blob_id) = version.blob_id else {
                continue;
            };
            files.push((entry.path_norm, blob_id));
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

        let mut files = Vec::new();
        for entry in scope_entries {
            if entry.entry_type != EntryKind::File {
                continue;
            }

            let history = self
                .entry_repo
                .get_entry_history(&entry.id, u32::MAX, None)
                .await?;
            let Some(version) = history.into_iter().find(|item| item.created_at <= cutoff) else {
                continue;
            };
            let Some(blob_id) = version.blob_id else {
                continue;
            };
            files.push((entry.path_norm, blob_id));
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
    ) -> DomainResult<Vec<u8>> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

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

                zip.start_file(zip_path, options)
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to create zip entry: {}", e),
                    })?;

                loop {
                    let read =
                        reader
                            .read(&mut buffer)
                            .await
                            .map_err(|e| DomainError::Internal {
                                message: format!("Failed to stream blob data: {}", e),
                            })?;
                    if read == 0 {
                        break;
                    }

                    zip.write_all(&buffer[..read])
                        .map_err(|e| DomainError::Internal {
                            message: format!("Failed to write zip entry: {}", e),
                        })?;
                }
            }

            zip.finish().map_err(|e| DomainError::Internal {
                message: format!("Failed to finalize zip archive: {}", e),
            })?;
        }

        Ok(cursor.into_inner())
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
        let bytes = self
            .build_directory_archive(&archive_name, path, files)
            .await?;

        Ok(DirectoryArchive {
            filename: format!("{}.zip", archive_name),
            bytes,
        })
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

    pub async fn move_entries(
        &self,
        namespace_id: &NamespaceId,
        sources: &[NormalizedPath],
        destination: &NormalizedPath,
        message: Option<&str>,
        user_id: &UserId,
    ) -> DomainResult<MutationResult> {
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

        let destination_entry = if destination.as_str().is_empty() {
            None
        } else {
            self.entry_repo
                .find_by_path(namespace_id, destination)
                .await?
        };
        let destination_is_directory = destination.as_str().is_empty()
            || matches!(
                destination_entry.as_ref().map(|entry| entry.entry_type),
                Some(EntryKind::Directory)
            );

        if sources.len() > 1 && !destination_is_directory {
            return Err(DomainError::Validation {
                message: "Destination must be an existing directory for multiple sources"
                    .to_string(),
            });
        }

        let mut moving_entries = Vec::new();
        let mut changed_entries = Vec::new();

        for source in sources {
            let source_entry = self
                .entry_repo
                .find_by_path(namespace_id, source)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    resource: format!("entry {}", source.as_str()),
                })?;

            let target_root = if sources.len() == 1 && !destination_is_directory {
                destination.clone()
            } else {
                join_path(destination, basename(source))?
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

            let subtree =
                collect_descendants(&self.entry_repo, namespace_id, &source_entry).await?;
            for entry in subtree {
                let new_path = compute_move_target(source, &entry.path_norm, &target_root)?;
                moving_entries.push((entry, new_path));
            }
        }

        let original_paths = moving_entries
            .iter()
            .map(|(entry, _)| entry.path_norm.as_str().to_string())
            .collect::<std::collections::HashSet<String>>();

        let candidates = moving_entries
            .iter()
            .filter(|(_, new_path)| !original_paths.contains(new_path.as_str()))
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

        // 单事务批量更新路径，避免逐条提交。
        let moves = moving_entries
            .iter()
            .map(|(entry, new_path)| (entry.id, new_path.clone()))
            .collect::<Vec<_>>();
        self.entry_repo.move_entries(&moves).await?;

        for (entry, new_path) in &moving_entries {
            changed_entries.push(ChangedEntry {
                entry_id: entry.id,
                path: new_path.as_str().to_string(),
                kind: entry.entry_type,
                current_version_id: entry.current_version_id,
                change_type: ChangeType::Renamed,
            });
        }
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

    pub async fn delete_entries(
        &self,
        namespace_id: &NamespaceId,
        paths: &[NormalizedPath],
        message: Option<&str>,
        user_id: &UserId,
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

        self.entry_repo.delete_entries(&entry_ids).await?;

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
    pub mutation: MutationResult,
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
    E: EntryRepo,
    S: SnapshotRepo,
    B: BlobStore,
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
        let _ = mime_type;
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
                size_bytes,
                chunk_size,
                user_id,
            )
            .await?;

        self.get_upload_status(&upload_id).await
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
        self.commit_upload_stream(session, upload_stream, expected_sha256, message)
            .await
    }

    pub async fn complete_upload_from_stream(
        &self,
        upload_id: &UploadId,
        expected_sha256: Option<&str>,
        message: Option<&str>,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> DomainResult<UploadCompleteResponse> {
        let session = self.upload_store.get_upload_session(upload_id).await?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(DomainError::UploadExpired);
        }

        self.commit_upload_stream(session, upload_stream, expected_sha256, message)
            .await
    }

    async fn commit_upload_stream(
        &self,
        session: UploadSession,
        upload_stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        expected_sha256: Option<&str>,
        message: Option<&str>,
    ) -> DomainResult<UploadCompleteResponse> {
        let (blob_id, content_hash, created_blob, stored_size) = self
            .blob_store
            .store_blob_stream(upload_stream, expected_sha256)
            .await?;

        if stored_size != session.declared_size.as_u64() {
            if created_blob {
                let _ = self.blob_store.delete_blob(&blob_id).await;
            }
            return Err(DomainError::UploadConflict);
        }

        let file_path = uploaded_file_path(&session.target_path_norm, &session.filename)?;
        let mut changed_entries = ensure_directory_path(
            &self.entry_repo,
            &session.namespace_id,
            &session.target_path_norm,
            &session.owner_user_id,
        )
        .await?;
        let entry_id = if let Some(entry) = self
            .entry_repo
            .find_by_path(&session.namespace_id, &file_path)
            .await?
        {
            if entry.entry_type != EntryKind::File {
                return Err(DomainError::PathConflict {
                    message: format!("Path is occupied by a directory: {}", file_path.as_str()),
                });
            }
            entry.id
        } else {
            self.entry_repo
                .create_entry(
                    &session.namespace_id,
                    &file_path,
                    EntryKind::File,
                    &session.owner_user_id,
                )
                .await?
        };

        let mime_type = guess_mime_type(&session.filename);
        let normalized_message = normalize_message(message);
        let version_result = self
            .entry_repo
            .create_version(
                &entry_id,
                Some(&blob_id),
                Some(&content_hash),
                stored_size,
                mime_type.as_deref(),
                &session.owner_user_id,
                normalized_message.as_deref(),
            )
            .await;
        let version = match version_result {
            Ok(version) => version,
            Err(err) => {
                if created_blob {
                    let _ = self.blob_store.delete_blob(&blob_id).await;
                }
                return Err(err);
            }
        };

        self.upload_store
            .complete_upload_session(&session.id)
            .await?;
        let upload = self.upload_store.get_upload_session(&session.id).await?;
        if let Err(err) = self.upload_store.cancel_upload_session(&session.id).await {
            tracing::warn!(
                upload_id = %session.id,
                error = ?err,
                "failed to cleanup completed upload session"
            );
        }
        let entry = self
            .entry_repo
            .find_by_path(&session.namespace_id, &file_path)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                resource: "entry".to_string(),
            })?;
        let snapshot_entries =
            collect_snapshot_state(&self.entry_repo, &session.namespace_id, Vec::new()).await?;

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

        let mutation = finalize_mutation(
            &self.snapshot_repo,
            &session.namespace_id,
            message,
            &session.owner_user_id,
            changed_entries,
            snapshot_entries,
            Vec::new(),
        )
        .await?;

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
    E: EntryRepo,
    S: SnapshotRepo,
    B: BlobStore,
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

        // Sort by score (descending)
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply limit after combining results
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
    E: EntryRepo + Clone,
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

    pub async fn disable_share(&self, code: &str, user_id: &UserId) -> DomainResult<()> {
        let share = self.share_repo.find_share_by_code(code).await?;

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

        let user_summaries = users
            .into_iter()
            .map(|user| AdminUserSummary {
                id: user.id,
                username: user.username.as_str().to_string(),
                email: user.email.as_ref().map(|email| email.as_str().to_string()),
                role: user.role,
                disabled: user.disabled,
                created_at: user.created_at,
                last_login: None, // TODO: implement last login tracking
            })
            .collect();

        Ok(AdminUserList {
            users: user_summaries,
            total_count,
            page,
            page_size,
        })
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
        } = req;

        // Check if user exists
        let current_user = self.auth_service.user_repo().find_by_id(user_id).await?;

        self.ensure_role_guardrails_on_update(&current_user, role, disabled)
            .await?;

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
                Some(&first.mutation.snapshot_id),
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

        let snapshot_commit = first.mutation.snapshot_id.to_string();
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
        let mut live_zip = zip::ZipArchive::new(std::io::Cursor::new(live_archive.bytes))
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
        let mut first_version_zip =
            zip::ZipArchive::new(std::io::Cursor::new(first_version_archive.bytes))
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
                Some(first.mutation.snapshot_id.to_string().as_str()),
            )
            .await
            .expect("snapshot directory archive should build");
        let mut snapshot_zip = zip::ZipArchive::new(std::io::Cursor::new(snapshot_archive.bytes))
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

        // 孤儿文件：只有磁盘文件、没有元数据行（模拟上传中断）
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

        // 刚写入的孤儿文件应受保护期保护
        let (fresh_id, _, _) = blob_store
            .store_blob(b"fresh-orphan", None)
            .await
            .expect("blob should store");

        let snapshot_repo = vfiles_infra_sqlite::SqliteSnapshotRepo::new(pool.clone());
        let service =
            MaintenanceService::new(blob_store.clone(), entry_repo.clone(), snapshot_repo);
        let report = service
            .purge_orphan_blobs(3600)
            .await
            .expect("purge should succeed");

        assert_eq!(report.removed, 1);
        assert!(report.freed_bytes > 0);
        assert!(!orphan_path.exists(), "orphan file should be removed");
        assert!(
            blob_store
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
            .run_once(3600, 1)
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
            .run_once(3600, 0)
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
