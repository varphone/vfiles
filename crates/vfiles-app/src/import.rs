//! 批量导入：把「多个文件写入」合并为**一次**快照提交。
//!
//! 单文件上传（HTTP）走 `UploadService`，每次写入都会写一份全命名空间快照；
//! 对批量导入（FTP、目录导入等）会产生 O(文件数 × 条目数) 的写入量。这里把变更
//! 累积到批次里，按数量阈值或会话结束时一次性提交，成本降为 O(条目数)。
//!
//! 语义说明：
//! - entry/version 在 `import_file_stream` 返回时即已落库（可被其它连接立刻读到）；
//! - 只有**快照**被延后到 `flush`，用于历史回放与变更记录；
//! - 批次未 flush 就中断时，数据仍然有效，只是这些变更会体现在下一次快照里。

use std::sync::Arc;

use tokio::io::AsyncReadExt;

use vfiles_domain::*;

use crate::services::{
    ChangedEntry, MutationResult, collect_snapshot_state, ensure_directory_path, finalize_mutation,
    guess_mime_type, normalize_message,
};

/// 单个文件导入结果。
#[derive(Debug)]
pub struct ImportedFile {
    pub entry: Entry,
    /// 新写入的版本号。
    pub version_no: u32,
    pub version_id: VersionId,
    /// 本次是否真的写入了新的 blob（同内容命中去重时为 false）。
    pub blob_created: bool,
    pub size_bytes: u64,
    pub content_hash: String,
    /// 内容与当前版本一致且开启了 `skip_unchanged`，未生成新版本。
    pub unchanged: bool,
}

/// 目录导入结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportedDirectory {
    /// 新建了目录
    Created,
    /// 目录已存在
    Existing,
}

/// 批量导入的提交策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotMode {
    /// 每次写入都提交快照，与 HTTP 上传行为一致（最慢，历史最细）。
    PerFile,
    /// 按阈值合并提交（默认）。
    Batch,
    /// 导入完全不产生快照（最快，仅记录 entry/version）。
    Off,
}

/// 批量导入会话。
///
/// 与 `UploadService` 的差别：没有 upload session、无需预先声明大小（FTP 的
/// `STOR` 不提供大小），大小上限在流式读取时通过 `take` 截断并校验。
pub struct ImportBatch {
    entry_repo: Arc<dyn EntryRepo + Send + Sync>,
    snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
    blob_store: Arc<dyn BlobStore + Send + Sync>,
    namespace_id: NamespaceId,
    actor_user_id: UserId,
    /// 快照消息前缀，例如 "FTP 导入"；实际消息会带上文件数。
    label: String,
    snapshot_mode: SnapshotMode,
    flush_threshold: usize,
    /// 内容与当前版本一致时跳过（不生成新版本），CLI 导入默认开启
    skip_unchanged: bool,
    changed: Vec<ChangedEntry>,
    /// 自上次提交以来导入的文件数（阈值判定按文件计数，而不是变更项数，
    /// 否则「隐式建目录」也会被算进阈值，导致提交时机不可预期）。
    files_since_flush: u64,
    files_imported: u64,
    unchanged_files: u64,
    directories_imported: u64,
    bytes_imported: u64,
    snapshots_written: u64,
}

impl std::fmt::Debug for ImportBatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportBatch")
            .field("namespace_id", &self.namespace_id)
            .field("actor_user_id", &self.actor_user_id)
            .field("snapshot_mode", &self.snapshot_mode)
            .field("files_imported", &self.files_imported)
            .field("bytes_imported", &self.bytes_imported)
            .field("snapshots_written", &self.snapshots_written)
            .finish_non_exhaustive()
    }
}

impl ImportBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        entry_repo: Arc<dyn EntryRepo + Send + Sync>,
        snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
        blob_store: Arc<dyn BlobStore + Send + Sync>,
        namespace_id: NamespaceId,
        actor_user_id: UserId,
        label: impl Into<String>,
        snapshot_mode: SnapshotMode,
        flush_threshold: usize,
    ) -> Self {
        Self::with_options(
            entry_repo,
            snapshot_repo,
            blob_store,
            namespace_id,
            actor_user_id,
            label,
            snapshot_mode,
            flush_threshold,
            false,
        )
    }

    /// 与 [`ImportBatch::new`] 相同，但可指定「内容未变化时跳过」。
    #[allow(clippy::too_many_arguments)]
    pub fn with_options(
        entry_repo: Arc<dyn EntryRepo + Send + Sync>,
        snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
        blob_store: Arc<dyn BlobStore + Send + Sync>,
        namespace_id: NamespaceId,
        actor_user_id: UserId,
        label: impl Into<String>,
        snapshot_mode: SnapshotMode,
        flush_threshold: usize,
        skip_unchanged: bool,
    ) -> Self {
        Self {
            entry_repo,
            snapshot_repo,
            blob_store,
            namespace_id,
            actor_user_id,
            label: label.into(),
            snapshot_mode,
            flush_threshold: flush_threshold.max(1),
            skip_unchanged,
            changed: Vec::new(),
            files_since_flush: 0,
            files_imported: 0,
            unchanged_files: 0,
            directories_imported: 0,
            bytes_imported: 0,
            snapshots_written: 0,
        }
    }

    pub fn files_imported(&self) -> u64 {
        self.files_imported
    }

    /// 因内容未变化而跳过的文件数（仅 `skip_unchanged` 时可能非零）。
    pub fn unchanged_files(&self) -> u64 {
        self.unchanged_files
    }

    /// 本次批次新建的目录数。
    pub fn directories_imported(&self) -> u64 {
        self.directories_imported
    }

    pub fn bytes_imported(&self) -> u64 {
        self.bytes_imported
    }

    pub fn snapshots_written(&self) -> u64 {
        self.snapshots_written
    }

    /// 待提交（尚未进入快照）的变更数。
    pub fn pending_changes(&self) -> usize {
        self.changed.len()
    }

    /// 是否有未提交的变更。
    pub fn has_pending(&self) -> bool {
        !self.changed.is_empty()
    }

    /// 流式导入一个文件到 `path`。
    ///
    /// - `max_bytes` 为 `Some` 时，读取 `max_bytes + 1` 字节即可判定超限并回滚；
    /// - `message` 记录到版本上（单文件粒度）；
    /// - `SnapshotMode::PerFile` 或达到阈值时自动提交一次快照。
    pub async fn import_file_stream(
        &mut self,
        path: &NormalizedPath,
        reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        max_bytes: Option<u64>,
        message: Option<&str>,
    ) -> DomainResult<ImportedFile> {
        if path.as_str().is_empty() {
            return Err(DomainError::Validation {
                message: "Import path must not be empty".to_string(),
            });
        }

        // 流式写入 blob（sha256 去重）；多读 1 字节用于识别超限
        let reader: Box<dyn tokio::io::AsyncRead + Send + Unpin> = match max_bytes {
            Some(limit) => Box::new(reader.take(limit.saturating_add(1))),
            None => reader,
        };
        let (blob_id, content_hash, blob_created, stored_size) = self
            .blob_store
            .store_blob_stream(reader, None, None, None)
            .await?;

        if let Some(limit) = max_bytes
            && stored_size > limit
        {
            if blob_created {
                let _ = self.blob_store.delete_blob(&blob_id).await;
            }
            return Err(DomainError::StorageQuotaExceeded);
        }

        let parent_path = path
            .as_str()
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        let filename = path
            .as_str()
            .rsplit_once('/')
            .map(|(_, name)| name)
            .unwrap_or_else(|| path.as_str());
        crate::services::validate_filename(filename)?;
        let parent_path =
            NormalizedPath::new(parent_path).map_err(|_| DomainError::Validation {
                message: "Invalid parent path".to_string(),
            })?;

        let changed_directories = ensure_directory_path(
            &*self.entry_repo,
            &self.namespace_id,
            &parent_path,
            &self.actor_user_id,
        )
        .await?;

        let entry = match self
            .entry_repo
            .find_by_path(&self.namespace_id, path)
            .await?
        {
            Some(existing) if existing.entry_type != EntryKind::File => {
                if blob_created {
                    let _ = self.blob_store.delete_blob(&blob_id).await;
                }
                return Err(DomainError::PathConflict {
                    message: format!("Path is occupied by a directory: {}", path.as_str()),
                });
            }
            Some(existing) => existing,
            None => {
                // create_entry 只返回 id，随后重新读取完整条目（与 HTTP 上传一致）
                if let Err(err) = self
                    .entry_repo
                    .create_entry(
                        &self.namespace_id,
                        path,
                        EntryKind::File,
                        &self.actor_user_id,
                    )
                    .await
                {
                    if blob_created {
                        let _ = self.blob_store.delete_blob(&blob_id).await;
                    }
                    return Err(err);
                }

                self.entry_repo
                    .find_by_path(&self.namespace_id, path)
                    .await?
                    .ok_or_else(|| DomainError::NotFound {
                        resource: "entry".to_string(),
                    })?
            }
        };

        // 内容与当前版本一致时跳过（CLI 目录导入默认开启，避免重复导入污染历史）
        let current_version = match entry.current_version_id {
            Some(version_id) => self.entry_repo.find_version(&version_id).await.ok(),
            None => None,
        };
        let unchanged = self.skip_unchanged
            && current_version
                .as_ref()
                .is_some_and(|version| version.blob_id.as_ref() == Some(&blob_id));

        if unchanged {
            // 新建的父目录仍属于本次导入，需要进入快照
            self.changed.extend(changed_directories);
            self.unchanged_files += 1;
            let version = current_version.expect("unchanged implies an existing version");
            return Ok(ImportedFile {
                entry,
                version_no: version.version_no,
                version_id: version.id,
                blob_created,
                size_bytes: stored_size,
                content_hash: content_hash.as_str().to_string(),
                unchanged: true,
            });
        }

        let mime_type = guess_mime_type(filename);
        let normalized_message = normalize_message(message);
        let version = match self
            .entry_repo
            .create_version(
                &entry.id,
                Some(&blob_id),
                Some(&content_hash),
                stored_size,
                mime_type.as_deref(),
                &self.actor_user_id,
                normalized_message.as_deref(),
            )
            .await
        {
            Ok(version) => version,
            Err(err) => {
                if blob_created {
                    let _ = self.blob_store.delete_blob(&blob_id).await;
                }
                return Err(err);
            }
        };

        self.changed.extend(changed_directories);
        self.changed.push(ChangedEntry {
            entry_id: entry.id,
            path: path.as_str().to_string(),
            kind: EntryKind::File,
            current_version_id: Some(version.id),
            change_type: if version.version_no == 1 {
                ChangeType::Added
            } else {
                ChangeType::Modified
            },
        });
        self.files_imported += 1;
        self.files_since_flush += 1;
        self.bytes_imported += stored_size;
        if self.snapshot_mode == SnapshotMode::Off {
            // 不产生快照时无需保留变更列表，避免大批量导入时空耗内存
            self.changed.clear();
        }

        let should_flush = match self.snapshot_mode {
            SnapshotMode::PerFile => true,
            SnapshotMode::Batch => self.files_since_flush >= self.flush_threshold as u64,
            SnapshotMode::Off => false,
        };
        if should_flush {
            self.flush().await?;
        }

        Ok(ImportedFile {
            entry,
            version_no: version.version_no,
            version_id: version.id,
            blob_created,
            size_bytes: stored_size,
            content_hash: content_hash.as_str().to_string(),
            unchanged: false,
        })
    }

    /// 在批次内创建目录（父目录按需补齐），不单独提交快照。
    pub async fn create_directory(
        &mut self,
        path: &NormalizedPath,
    ) -> DomainResult<ImportedDirectory> {
        if path.as_str().is_empty() {
            return Err(DomainError::Validation {
                message: "Directory path must not be empty".to_string(),
            });
        }

        if let Some(existing) = self
            .entry_repo
            .find_by_path(&self.namespace_id, path)
            .await?
        {
            return if existing.entry_type == EntryKind::Directory {
                Ok(ImportedDirectory::Existing)
            } else {
                Err(DomainError::PathConflict {
                    message: format!("Path is occupied by a file: {}", path.as_str()),
                })
            };
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

        let changed_directories = ensure_directory_path(
            &*self.entry_repo,
            &self.namespace_id,
            &parent_path,
            &self.actor_user_id,
        )
        .await?;

        let entry_id = self
            .entry_repo
            .create_entry(
                &self.namespace_id,
                path,
                EntryKind::Directory,
                &self.actor_user_id,
            )
            .await?;

        self.changed.extend(changed_directories);
        self.changed.push(ChangedEntry {
            entry_id,
            path: path.as_str().to_string(),
            kind: EntryKind::Directory,
            current_version_id: None,
            change_type: ChangeType::Added,
        });
        self.directories_imported += 1;

        Ok(ImportedDirectory::Created)
    }

    /// 提交累积的变更（一次全量快照）。无待提交变更时返回 `None`。
    pub async fn flush(&mut self) -> DomainResult<Option<MutationResult>> {
        if self.changed.is_empty() || self.snapshot_mode == SnapshotMode::Off {
            self.changed.clear();
            self.files_since_flush = 0;
            return Ok(None);
        }

        let count = self.changed.len();
        let message = self.snapshot_message(count);
        let changed_entries = std::mem::take(&mut self.changed);
        let snapshot_entries =
            collect_snapshot_state(&*self.entry_repo, &self.namespace_id, Vec::new()).await?;
        let mutation = finalize_mutation(
            &*self.snapshot_repo,
            &self.namespace_id,
            Some(&message),
            &self.actor_user_id,
            changed_entries,
            snapshot_entries,
            Vec::new(),
        )
        .await?;
        self.snapshots_written += 1;
        self.files_since_flush = 0;
        Ok(Some(mutation))
    }

    /// 会话结束：提交剩余变更。
    pub async fn finish(&mut self) -> DomainResult<Option<MutationResult>> {
        self.flush().await
    }

    fn snapshot_message(&self, changed: usize) -> String {
        let files = self.files_imported;
        if files > 0 && changed >= files as usize {
            format!("{}（{} 个文件）", self.label, files)
        } else {
            format!("{}（{} 项变更）", self.label, changed)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use tempfile::TempDir;
    use vfiles_domain::{NamespaceRepo, UserRepo};
    use vfiles_infra_sqlite::{
        FsBlobStore, SqliteEntryRepo, SqliteMigrations, SqliteNamespaceRepo, SqlitePoolFactory,
        SqliteSnapshotRepo, SqliteUserRepo,
    };

    struct Fixture {
        _temp_dir: TempDir,
        pool: sqlx::SqlitePool,
        namespace_id: NamespaceId,
        user_id: UserId,
        entry_repo: SqliteEntryRepo,
        snapshot_repo: SqliteSnapshotRepo,
        blob_store: FsBlobStore,
    }

    impl Fixture {
        async fn new() -> Self {
            let temp_dir = tempfile::tempdir().expect("tempdir should be created");
            let storage_root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
                .expect("tempdir path should be valid utf-8");
            let pool = SqlitePoolFactory::connect(storage_root.join("vfiles.db").as_path())
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

            Self {
                entry_repo: SqliteEntryRepo::new(pool.clone()),
                snapshot_repo: SqliteSnapshotRepo::new(pool.clone()),
                blob_store: FsBlobStore::new(pool.clone(), storage_root.join("blobs")),
                _temp_dir: temp_dir,
                pool,
                namespace_id,
                user_id,
            }
        }

        fn batch(&self, mode: SnapshotMode, threshold: usize) -> ImportBatch {
            self.batch_with_options(mode, threshold, false)
        }

        fn batch_with_options(
            &self,
            mode: SnapshotMode,
            threshold: usize,
            skip_unchanged: bool,
        ) -> ImportBatch {
            ImportBatch::with_options(
                Arc::new(self.entry_repo.clone()),
                Arc::new(self.snapshot_repo.clone()),
                Arc::new(self.blob_store.clone()),
                self.namespace_id,
                self.user_id,
                "导入",
                mode,
                threshold,
                skip_unchanged,
            )
        }

        async fn entry_paths(&self) -> Vec<String> {
            let mut entries = self
                .entry_repo
                .find_all(&self.namespace_id)
                .await
                .expect("find_all should work");
            entries.sort_by(|a, b| a.path_norm.as_str().cmp(b.path_norm.as_str()));
            entries
                .into_iter()
                .map(|entry| entry.path_norm.as_str().to_string())
                .collect()
        }

        async fn snapshot_count(&self) -> i64 {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM snapshots")
                .fetch_one(&self.pool)
                .await
                .expect("snapshot count should be readable")
        }
    }

    fn path(value: &str) -> NormalizedPath {
        NormalizedPath::new(value).expect("path should be valid")
    }

    fn reader(data: &'static str) -> Box<dyn tokio::io::AsyncRead + Send + Unpin> {
        Box::new(std::io::Cursor::new(data.as_bytes().to_vec()))
    }

    #[tokio::test]
    async fn batch_mode_commits_one_snapshot_per_threshold() {
        let fixture = Fixture::new().await;
        let mut batch = fixture.batch(SnapshotMode::Batch, 2);

        for (index, name) in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]
            .iter()
            .enumerate()
        {
            let file_path = path(&format!("dir/{name}"));
            batch
                .import_file_stream(&file_path, reader("hello"), None, Some("测试导入"))
                .await
                .expect("import should succeed");
            // 阈值 2：第 2、4 个文件后各提交一次
            assert_eq!(
                fixture.snapshot_count().await,
                ((index as i64 + 1) / 2),
                "快照应按阈值合并提交"
            );
        }

        assert_eq!(batch.pending_changes(), 1, "最后一个文件尚未提交");
        batch.finish().await.expect("finish should succeed");

        // 5 个文件 / 阈值 2 => 3 次快照（2+2+1）
        assert_eq!(fixture.snapshot_count().await, 3);
        assert_eq!(batch.files_imported(), 5);
        assert_eq!(batch.snapshots_written(), 3);

        let entries = fixture
            .entry_repo
            .find_by_path(&fixture.namespace_id, &path("dir/e.txt"))
            .await
            .expect("entry lookup should succeed")
            .expect("file should exist");
        assert_eq!(entries.entry_type, EntryKind::File);
    }

    #[tokio::test]
    async fn per_file_mode_commits_each_import_and_off_mode_never_commits() {
        let fixture = Fixture::new().await;

        let mut per_file = fixture.batch(SnapshotMode::PerFile, 100);
        for name in ["a.txt", "b.txt"] {
            per_file
                .import_file_stream(&path(name), reader("x"), None, None)
                .await
                .expect("import should succeed");
        }
        assert_eq!(fixture.snapshot_count().await, 2);
        assert_eq!(per_file.pending_changes(), 0);

        let mut off = fixture.batch(SnapshotMode::Off, 100);
        for name in ["c.txt", "d.txt", "e.txt"] {
            off.import_file_stream(&path(name), reader("y"), None, None)
                .await
                .expect("import should succeed");
        }
        off.finish().await.expect("finish should succeed");
        assert_eq!(fixture.snapshot_count().await, 2, "Off 模式不应产生新快照");
        assert_eq!(off.pending_changes(), 0, "Off 模式不应在内存里累积变更");

        // 数据本身仍然可读
        assert!(
            fixture
                .entry_repo
                .find_by_path(&fixture.namespace_id, &path("e.txt"))
                .await
                .expect("lookup should succeed")
                .is_some()
        );
    }

    #[tokio::test]
    async fn oversized_stream_is_rejected_without_leaving_entry() {
        let fixture = Fixture::new().await;
        let mut batch = fixture.batch(SnapshotMode::Batch, 10);

        let result = batch
            .import_file_stream(&path("big.bin"), reader("0123456789"), Some(5), None)
            .await;

        assert!(
            matches!(result, Err(DomainError::StorageQuotaExceeded)),
            "超过上限应返回 StorageQuotaExceeded，实际: {result:?}"
        );
        assert!(
            fixture
                .entry_repo
                .find_by_path(&fixture.namespace_id, &path("big.bin"))
                .await
                .expect("lookup should succeed")
                .is_none(),
            "超限文件不应留下条目"
        );

        // 恰好等于上限则通过
        let ok = batch
            .import_file_stream(&path("exact.bin"), reader("01234"), Some(5), None)
            .await;
        assert!(ok.is_ok(), "等于上限应允许，实际: {ok:?}");
    }

    #[tokio::test]
    async fn identical_content_is_deduplicated_and_reimport_creates_new_version() {
        let fixture = Fixture::new().await;
        let mut batch = fixture.batch(SnapshotMode::Batch, 10);

        let first = batch
            .import_file_stream(&path("dup.txt"), reader("same"), None, None)
            .await
            .expect("first import should succeed");
        assert!(first.blob_created, "首次写入应创建 blob");
        assert_eq!(first.version_no, 1);

        let second = batch
            .import_file_stream(&path("copy.txt"), reader("same"), None, None)
            .await
            .expect("second import should succeed");
        assert!(!second.blob_created, "同内容应命中去重");

        let third = batch
            .import_file_stream(&path("dup.txt"), reader("same"), None, None)
            .await
            .expect("re-import should succeed");
        assert_eq!(third.version_no, 2, "同路径再次导入应生成新版本");
        assert!(!third.blob_created);
    }

    #[tokio::test]
    async fn skip_unchanged_avoids_new_versions_on_reimport() {
        let fixture = Fixture::new().await;
        let mut batch = fixture.batch(SnapshotMode::Batch, 10);

        let first = batch
            .import_file_stream(&path("same.txt"), reader("content"), None, None)
            .await
            .expect("first import should succeed");
        assert!(!first.unchanged);
        assert_eq!(first.version_no, 1);

        // 同样的内容 + 跳过未变更 ⇒ 不生成新版本
        let skipping = fixture.batch_with_options(SnapshotMode::Batch, 10, true);
        let mut skipping = skipping;
        let second = skipping
            .import_file_stream(&path("same.txt"), reader("content"), None, None)
            .await
            .expect("reimport should succeed");
        assert!(second.unchanged, "相同内容应被跳过");
        assert_eq!(second.version_no, 1, "不应生成新版本");
        assert_eq!(skipping.unchanged_files(), 1);
        assert_eq!(skipping.files_imported(), 0, "跳过的文件不计入导入数");

        // 内容变化时仍然写入新版本
        let third = skipping
            .import_file_stream(&path("same.txt"), reader("changed"), None, None)
            .await
            .expect("changed import should succeed");
        assert!(!third.unchanged);
        assert_eq!(third.version_no, 2);
    }

    #[tokio::test]
    async fn directories_are_created_within_the_batch() {
        let fixture = Fixture::new().await;
        let mut batch = fixture.batch(SnapshotMode::Batch, 100);

        // 先建两个空目录（含多级），再放一个文件
        assert_eq!(
            batch
                .create_directory(&path("empty"))
                .await
                .expect("create dir"),
            ImportedDirectory::Created
        );
        assert_eq!(
            batch
                .create_directory(&path("nested/deep"))
                .await
                .expect("create nested dir"),
            ImportedDirectory::Created
        );
        // 幂等：重复创建返回 Existing
        assert_eq!(
            batch
                .create_directory(&path("empty"))
                .await
                .expect("create dir twice"),
            ImportedDirectory::Existing
        );
        assert_eq!(
            batch.directories_imported(),
            2,
            "嵌套目录只计显式创建的那一个"
        );

        batch
            .import_file_stream(&path("empty/a.txt"), reader("x"), None, None)
            .await
            .expect("import into directory");

        // 整个批次只产生一次快照
        batch.finish().await.expect("finish");
        assert_eq!(fixture.snapshot_count().await, 1);

        let entries = fixture.entry_paths().await;
        assert_eq!(
            entries,
            vec!["empty", "empty/a.txt", "nested", "nested/deep"],
            "空目录与嵌套目录都应落库"
        );
    }

    #[tokio::test]
    async fn importing_onto_a_directory_is_rejected() {
        let fixture = Fixture::new().await;
        let mut batch = fixture.batch(SnapshotMode::Batch, 10);

        batch
            .import_file_stream(&path("docs/keep.txt"), reader("x"), None, None)
            .await
            .expect("import should create the directory implicitly");

        let result = batch
            .import_file_stream(&path("docs"), reader("y"), None, None)
            .await;
        assert!(
            matches!(result, Err(DomainError::PathConflict { .. })),
            "与目录同名应返回 PathConflict，实际: {result:?}"
        );
    }
}
