//! FTP 存储后端：把 FTP 命令映射到 VFiles 的条目/版本/blob 模型。
//!
//! 设计要点：
//! - `put` 走 `ImportBatch`：`STOR` 在会话内累积变更，按阈值或会话结束一次性
//!   提交快照，避免大批量导入时每次写入都写一份全命名空间快照；
//! - `get` 复用 `WorkspaceService::open_file`（blob 流），并支持 `REST` 断点续传；
//! - `mkd/rmd/del/rename` 先提交挂起的批次，再调用既有工作区服务，语义与 Web 端一致；
//! - 路径全部落在登录用户自己的命名空间内，无法越权。

use std::{
    collections::HashMap,
    fmt,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex as SyncMutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use tokio::sync::Mutex as AsyncMutex;

use async_trait::async_trait;
use tokio::io::{AsyncRead, ReadBuf};
use tracing::{debug, warn};
use unftp_core::storage::{
    Error, ErrorKind, FEATURE_RESTART, Fileinfo, Metadata, Result, StorageBackend,
};
use vfiles_app::{
    DefaultWorkspaceService, ImportBatch, IngestStats, MoveSubtreeLimits, SnapshotMode, TreeItem,
};
use vfiles_domain::{
    BlobStore, DomainError, EntryKind, EntryRepo, EntryWriteCondition, NamespaceId, NormalizedPath,
    SnapshotRepo, UserRepo, WebdavLockRepo,
};

use crate::auth::VfilesFtpUser;
use crate::error::{to_ftp_error, transfer_aborted};
use crate::path::{to_client_path, to_normalized};

/// 单次目录列表按较小页面读取，以限制构建结果期间的瞬时内存。
const LIST_PAGE_SIZE: u32 = 100;
/// libunftp 的存储接口要求返回完整 Vec，因此限制单次目录列表的总量。
const MAX_LIST_ENTRIES: u64 = 20_000;
const MAX_LIST_METADATA_BYTES: usize = 8 * 1024 * 1024;
/// RNTO builds source, destination and lock snapshots for the whole moved subtree.
const MAX_RENAME_SUBTREE_ENTRIES: usize = 20_000;
const MAX_RENAME_SUBTREE_PATH_BYTES: usize = 4 * 1024 * 1024;
const FTP_ACCOUNT_RECHECK_INTERVAL: Duration = Duration::from_secs(5);
const FTP_ACCOUNT_RECHECK_TIMEOUT: Duration = Duration::from_secs(5);

type AccountCheckFuture = Pin<Box<dyn Future<Output = std::io::Result<()>> + Send>>;

struct AccountRevalidatingReader<R> {
    inner: R,
    user_repo: Arc<dyn UserRepo + Send + Sync>,
    user: VfilesFtpUser,
    next_check: Pin<Box<tokio::time::Sleep>>,
    pending_check: SyncMutex<Option<AccountCheckFuture>>,
    at_eof: bool,
    eof_validated: bool,
}

impl<R> AccountRevalidatingReader<R> {
    fn new(inner: R, user_repo: Arc<dyn UserRepo + Send + Sync>, user: VfilesFtpUser) -> Self {
        Self {
            inner,
            user_repo,
            user,
            next_check: Box::pin(tokio::time::sleep(FTP_ACCOUNT_RECHECK_INTERVAL)),
            pending_check: SyncMutex::new(None),
            at_eof: false,
            eof_validated: false,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for AccountRevalidatingReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.as_mut().get_mut();
        loop {
            if this.eof_validated {
                return Poll::Ready(Ok(()));
            }

            let check_due = this.at_eof || this.next_check.as_mut().poll(cx).is_ready();
            let needs_check = {
                let pending = this
                    .pending_check
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                pending.is_none() && check_due
            };
            if needs_check {
                let user_repo = Arc::clone(&this.user_repo);
                let user = this.user.clone();
                let future = async move {
                    match validate_ftp_user_current(user_repo.as_ref(), &user).await {
                        Ok(()) => Ok(()),
                        Err(_) => Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "FTP account is no longer authorized",
                        )),
                    }
                };
                let mut pending = this
                    .pending_check
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *pending = Some(Box::pin(future));
            }

            let check_result = {
                let mut pending = this
                    .pending_check
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(future) = pending.as_mut() {
                    match future.as_mut().poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(result) => {
                            *pending = None;
                            Some(result)
                        }
                    }
                } else {
                    None
                }
            };

            if let Some(result) = check_result {
                if let Err(err) = result {
                    return Poll::Ready(Err(err));
                }
                this.next_check
                    .as_mut()
                    .reset(tokio::time::Instant::now() + FTP_ACCOUNT_RECHECK_INTERVAL);
                if this.at_eof {
                    this.eof_validated = true;
                    return Poll::Ready(Ok(()));
                }
                continue;
            }

            let filled_before = buf.filled().len();
            let remaining_before = buf.remaining();
            match Pin::new(&mut this.inner).poll_read(cx, buf) {
                Poll::Ready(Ok(()))
                    if remaining_before > 0 && buf.filled().len() == filled_before =>
                {
                    this.at_eof = true;
                }
                result => return result,
            }
        }
    }
}

/// FTP 文件元数据。
#[derive(Debug, Clone, Copy)]
pub struct VfilesMetadata {
    size: u64,
    is_dir: bool,
    modified: SystemTime,
}

impl VfilesMetadata {
    fn new(size: u64, is_dir: bool, modified: SystemTime) -> Self {
        Self {
            size,
            is_dir,
            modified,
        }
    }
}

impl Metadata for VfilesMetadata {
    fn len(&self) -> u64 {
        self.size
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn is_file(&self) -> bool {
        !self.is_dir
    }

    fn is_symlink(&self) -> bool {
        false
    }

    fn modified(&self) -> Result<SystemTime> {
        Ok(self.modified)
    }

    fn gid(&self) -> u32 {
        0
    }

    fn uid(&self) -> u32 {
        0
    }
}

async fn validate_ftp_user_current(
    user_repo: &(dyn UserRepo + Send + Sync),
    user: &VfilesFtpUser,
) -> vfiles_domain::DomainResult<()> {
    if user.anonymous {
        return Ok(());
    }

    let current = tokio::time::timeout(FTP_ACCOUNT_RECHECK_TIMEOUT, user_repo.find_by_id(&user.id))
        .await
        .map_err(|_| DomainError::Internal {
            message: "FTP 账户状态校验超时".to_string(),
        })??;
    if current.disabled
        || current.username.as_str() != user.username
        || current.role != user.role
        || current.updated_at != user.account_updated_at
        || current.password_changed_at != user.password_changed_at
    {
        return Err(DomainError::SessionRevoked);
    }

    Ok(())
}

/// 后端共享依赖（每个会话克隆一份，内部用 `Arc` 共享）。
#[derive(Clone)]
pub struct BackendDeps {
    pub workspace: Arc<DefaultWorkspaceService>,
    pub entry_repo: Arc<dyn EntryRepo + Send + Sync>,
    pub snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
    pub blob_store: Arc<dyn BlobStore + Send + Sync>,
    pub user_repo: Arc<dyn UserRepo + Send + Sync>,
    pub lock_repo: Arc<dyn WebdavLockRepo>,
    pub stats: Arc<IngestStats>,
    /// 单文件大小上限（`None` 表示不限制）。
    pub max_file_size_bytes: Option<u64>,
    pub snapshot_mode: SnapshotMode,
    pub flush_threshold: usize,
}

impl fmt::Debug for BackendDeps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackendDeps")
            .field("max_file_size_bytes", &self.max_file_size_bytes)
            .field("snapshot_mode", &self.snapshot_mode)
            .field("flush_threshold", &self.flush_threshold)
            .finish_non_exhaustive()
    }
}

/// 每个登录会话一个实例：批次状态挂在实例上（libunftp 每个会话构造一次后端）。
pub struct VfilesStorageBackend {
    deps: BackendDeps,
    /// Keeps the FTP connection slot occupied while `Drop` commits a pending batch.
    session_permit: Option<Arc<tokio::sync::OwnedSemaphorePermit>>,
    /// 当前会话的导入批次（`enter` 之后可用）；与 `Drop` 的收尾任务共享。
    ///
    /// 用异步锁：批次操作是 async 且必须在持锁期间 await（否则并发 put 会丢变更）。
    batch: Arc<AsyncMutex<Option<ImportBatch>>>,
    /// 已提交的快照数，用于统计增量。
    snapshots_committed: Arc<AsyncMutex<u64>>,
    /// `active_sessions` 只统计完成认证并进入后端的会话。
    session_started: bool,
    /// 保证会话结束统计只记一次。
    session_finished: Arc<AtomicBool>,
}

impl fmt::Debug for VfilesStorageBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VfilesStorageBackend")
            .finish_non_exhaustive()
    }
}

impl VfilesStorageBackend {
    pub fn new(deps: BackendDeps) -> Self {
        Self::new_with_session_permit(deps, None)
    }

    pub(crate) fn new_with_session_permit(
        deps: BackendDeps,
        session_permit: Option<Arc<tokio::sync::OwnedSemaphorePermit>>,
    ) -> Self {
        Self {
            deps,
            session_permit,
            batch: Arc::new(AsyncMutex::new(None)),
            snapshots_committed: Arc::new(AsyncMutex::new(0)),
            session_started: false,
            session_finished: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn batch(&self) -> tokio::sync::MutexGuard<'_, Option<ImportBatch>> {
        self.batch.lock().await
    }

    /// 提交挂起的导入变更（一次快照）。
    ///
    /// `mkd/rmd/del/rename` 之前调用，保证快照顺序与客户端观察到的操作顺序一致。
    async fn flush_batch(&self) -> Result<()> {
        let mut guard = self.batch().await;
        let Some(batch) = guard.as_mut() else {
            return Ok(());
        };
        if !batch.has_pending() {
            return Ok(());
        }

        match batch.flush().await {
            Ok(_) => {
                let written = batch.snapshots_written();
                let mut committed = self.snapshots_committed.lock().await;
                if written > *committed {
                    self.deps.stats.record_snapshot_flush(written - *committed);
                    *committed = written;
                }
                Ok(())
            }
            Err(err) => {
                self.deps.stats.record_error();
                Err(to_ftp_error(err))
            }
        }
    }

    async fn import_file<R>(
        &self,
        user: &VfilesFtpUser,
        path: &NormalizedPath,
        input: R,
        conditions: &[EntryWriteCondition],
    ) -> Result<u64>
    where
        R: AsyncRead + Send + Sync + Unpin + 'static,
    {
        let mut guard = self.batch().await;
        let Some(batch) = guard.as_mut() else {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "会话未完成认证，无法上传",
            ));
        };

        let name = path.as_str().rsplit('/').next().unwrap_or(path.as_str());
        let message = format!("FTP 上传: {name}");
        let user_repo = Arc::clone(&self.deps.user_repo);
        let expected_user = user.clone();
        let before_commit =
            move || -> Pin<Box<dyn Future<Output = vfiles_domain::DomainResult<()>> + Send>> {
                Box::pin(async move {
                    validate_ftp_user_current(user_repo.as_ref(), &expected_user).await
                })
            };

        let result = batch
            .import_file_stream_with_conditions_and_precommit(
                path,
                Box::new(input),
                self.deps.max_file_size_bytes,
                Some(&message),
                conditions,
                before_commit,
            )
            .await;

        match result {
            Ok(imported) => {
                let written = batch.snapshots_written();
                let mut committed = self.snapshots_committed.lock().await;
                if written > *committed {
                    self.deps.stats.record_snapshot_flush(written - *committed);
                    *committed = written;
                }
                drop(committed);
                self.deps.stats.record_upload(imported.size_bytes);
                Ok(imported.size_bytes)
            }
            Err(err) => {
                self.deps.stats.record_error();
                Err(to_ftp_error(err))
            }
        }
    }

    async fn expected_unlocked_tokens(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> Result<Vec<String>> {
        let locks = self
            .deps
            .lock_repo
            .find_active_covering(namespace_id, path.as_str(), Self::lock_now_ms())
            .await
            .map_err(to_ftp_error)?;
        if locks.is_some() {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "资源受 WebDAV 写锁保护",
            ));
        }
        Ok(Vec::new())
    }

    fn lock_now_ms() -> i64 {
        time::OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .div_euclid(1_000_000)
            .clamp(0, i64::MAX as i128) as i64
    }

    fn entry_metadata(entry: &TreeItem) -> VfilesMetadata {
        let modified = SystemTime::from(entry.modified_at.unwrap_or(entry.created_at));
        VfilesMetadata::new(
            entry.size_bytes.unwrap_or(0),
            matches!(entry.kind, EntryKind::Directory),
            modified,
        )
    }

    /// 分页取全量子条目（FTP 的 `LIST` 没有分页语义）。
    async fn list_all(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> Result<Vec<TreeItem>> {
        let mut items = Vec::new();
        let mut metadata_bytes = 0usize;
        let mut offset = 0_u32;
        loop {
            let (page, total) = self
                .deps
                .workspace
                .live_children_page(namespace_id, path, LIST_PAGE_SIZE, offset)
                .await
                .map_err(to_ftp_error)?;
            if total > MAX_LIST_ENTRIES {
                return Err(Error::new(
                    ErrorKind::LocalError,
                    "FTP 目录列表超过安全上限",
                ));
            }

            let next_count = (items.len() as u64).saturating_add(page.len() as u64);
            let page_metadata_bytes = page.iter().fold(0usize, |total, item| {
                total.saturating_add(tree_item_memory_hint(item))
            });
            let next_metadata_bytes = metadata_bytes.saturating_add(page_metadata_bytes);
            if next_count > MAX_LIST_ENTRIES || next_metadata_bytes > MAX_LIST_METADATA_BYTES {
                return Err(Error::new(
                    ErrorKind::LocalError,
                    "FTP 目录列表超过安全上限",
                ));
            }

            let page_len = page.len() as u32;
            items.extend(page);
            metadata_bytes = next_metadata_bytes;
            offset += page_len;
            if page_len == 0 || u64::from(offset) >= total {
                break;
            }
        }
        Ok(items)
    }

    async fn find_entry(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> Result<Option<vfiles_domain::Entry>> {
        self.deps
            .entry_repo
            .find_by_path(namespace_id, path)
            .await
            .map_err(to_ftp_error)
    }

    async fn find_rename_subtree(
        &self,
        namespace_id: &NamespaceId,
        root: &NormalizedPath,
    ) -> Result<Vec<vfiles_domain::Entry>> {
        let mut entries = Vec::new();
        let mut path_bytes = 0usize;
        let mut after_path: Option<String> = None;

        loop {
            let remaining_with_overflow = MAX_RENAME_SUBTREE_ENTRIES
                .saturating_add(1)
                .saturating_sub(entries.len());
            let limit = remaining_with_overflow.min(LIST_PAGE_SIZE as usize) as u32;
            let page = self
                .deps
                .entry_repo
                .find_subtree_page(namespace_id, root, after_path.as_deref(), limit)
                .await
                .map_err(to_ftp_error)?;
            if page.is_empty() {
                break;
            }

            let next_count = entries.len().saturating_add(page.len());
            if next_count > MAX_RENAME_SUBTREE_ENTRIES {
                return Err(Error::new(
                    ErrorKind::LocalError,
                    "FTP 重命名目录超过安全条目上限",
                ));
            }
            let page_path_bytes = page.iter().fold(0usize, |total, entry| {
                total.saturating_add(entry.path_norm.as_str().len())
            });
            let next_path_bytes = path_bytes.saturating_add(page_path_bytes);
            if next_path_bytes > MAX_RENAME_SUBTREE_PATH_BYTES {
                return Err(Error::new(
                    ErrorKind::LocalError,
                    "FTP 重命名目录路径数据超过安全上限",
                ));
            }
            path_bytes = next_path_bytes;
            after_path = page
                .last()
                .map(|entry| entry.path_norm.as_str().to_string());
            entries.extend(page);
        }

        Ok(entries)
    }

    async fn ensure_user_current(&self, user: &VfilesFtpUser) -> Result<()> {
        validate_ftp_user_current(self.deps.user_repo.as_ref(), user)
            .await
            .map_err(to_ftp_error)
    }

    async fn validate_upload_path(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> Result<()> {
        let Some((parent, _)) = path.as_str().rsplit_once('/') else {
            return Ok(());
        };

        let mut current = String::new();
        let mut ancestors = Vec::new();
        for segment in parent.split('/') {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(segment);
            let current_path = NormalizedPath::new(&current)
                .map_err(|message| Error::new(ErrorKind::PermissionDenied, message))?;
            ancestors.push(current_path);
        }

        let mut checked_paths = ancestors.clone();
        checked_paths.push(path.clone());
        let entries = self
            .deps
            .entry_repo
            .find_paths(namespace_id, &checked_paths)
            .await
            .map_err(to_ftp_error)?;
        if entries.iter().any(|entry| {
            entry.path_norm.as_str() == path.as_str() && entry.entry_type != EntryKind::File
        }) {
            return Err(Error::new(
                ErrorKind::PermanentFileNotAvailable,
                "上传目标已是目录",
            ));
        }
        let file_paths = entries
            .into_iter()
            .filter(|entry| entry.entry_type != EntryKind::Directory)
            .map(|entry| entry.path_norm.as_str().to_string())
            .collect::<std::collections::HashSet<_>>();
        if ancestors
            .iter()
            .any(|ancestor| file_paths.contains(ancestor.as_str()))
        {
            return Err(Error::new(
                ErrorKind::PermanentFileNotAvailable,
                "上传路径中的父级不是目录",
            ));
        }
        Ok(())
    }

    /// 会话结束：提交剩余批次并记录汇总（幂等）。
    pub async fn finish_session(&self) {
        if let Err(err) = self.flush_batch().await {
            warn!(error = %err, "FTP 会话结束时提交批次失败");
        }
        if self.session_started && !self.session_finished.swap(true, Ordering::SeqCst) {
            self.deps.stats.session_finished();
        }
    }
}

/// 会话对象被丢弃（QUIT 或断开）时提交剩余批次。
///
/// libunftp 在会话结束时释放后端实例；`Drop` 里不能 await，因此把收尾工作
/// `spawn` 到当前运行时：取出批次、提交一次快照、递减活动会话计数。
impl Drop for VfilesStorageBackend {
    fn drop(&mut self) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            // 运行时已关闭（例如进程退出）：跳过收尾，数据本身已落库
            return;
        };

        let batch = Arc::clone(&self.batch);
        let committed = Arc::clone(&self.snapshots_committed);
        let finished = Arc::clone(&self.session_finished);
        let session_started = self.session_started;
        let stats = Arc::clone(&self.deps.stats);
        let session_permit = self.session_permit.clone();

        handle.spawn(async move {
            let _session_permit = session_permit;
            let mut guard = batch.lock().await;
            if let Some(batch) = guard.as_mut()
                && batch.has_pending()
            {
                match batch.flush().await {
                    Ok(_) => {
                        let written = batch.snapshots_written();
                        let mut committed = committed.lock().await;
                        if written > *committed {
                            stats.record_snapshot_flush(written - *committed);
                            *committed = written;
                        }
                    }
                    Err(err) => {
                        warn!(error = %err, "会话结束提交批次失败");
                        stats.record_error();
                    }
                }
            }
            if session_started && !finished.swap(true, Ordering::SeqCst) {
                stats.session_finished();
            }
        });
    }
}

#[async_trait]
impl StorageBackend<VfilesFtpUser> for VfilesStorageBackend {
    type Metadata = VfilesMetadata;

    fn name(&self) -> &str {
        "VFiles"
    }

    fn supported_features(&self) -> u32 {
        // 仅声明 RETR 的 REST 断点续传，不声明 SITE MD5
        FEATURE_RESTART
    }

    fn enter(&mut self, user_detail: &VfilesFtpUser) -> std::io::Result<()> {
        let batch = ImportBatch::new(
            Arc::clone(&self.deps.entry_repo),
            Arc::clone(&self.deps.snapshot_repo),
            Arc::clone(&self.deps.blob_store),
            user_detail.namespace_id,
            user_detail.id,
            "FTP 导入",
            self.deps.snapshot_mode,
            self.deps.flush_threshold,
        );
        // `enter` 在会话建立后立即调用，不存在锁竞争；用 try_lock 以避免同步上下文阻塞
        let mut guard = self
            .batch
            .try_lock()
            .map_err(|_| std::io::Error::other("导入批次状态不可用"))?;
        *guard = Some(batch);
        self.session_started = true;
        self.deps.stats.session_started();
        debug!(user = %user_detail.username, "FTP 会话已进入存储后端");
        Ok(())
    }

    async fn metadata<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &VfilesFtpUser,
        path: P,
    ) -> Result<Self::Metadata> {
        self.ensure_user_current(user).await?;
        let path = to_normalized(path.as_ref())?;

        if path.as_str().is_empty() {
            return Ok(VfilesMetadata::new(0, true, SystemTime::UNIX_EPOCH));
        }

        let entry = self
            .find_entry(&user.namespace_id, &path)
            .await?
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::PermanentFileNotAvailable,
                    format!("路径不存在: {}", path.as_str()),
                )
            })?;

        let (size, modified) = match (entry.entry_type, entry.current_version_id) {
            (EntryKind::File, Some(version_id)) => {
                let version = self
                    .deps
                    .entry_repo
                    .find_version(&version_id)
                    .await
                    .map_err(to_ftp_error)?;
                (
                    version.size_bytes.as_u64(),
                    SystemTime::from(version.created_at),
                )
            }
            _ => (0, SystemTime::from(entry.created_at)),
        };

        Ok(VfilesMetadata::new(
            size,
            matches!(entry.entry_type, EntryKind::Directory),
            modified,
        ))
    }

    async fn list<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &VfilesFtpUser,
        path: P,
    ) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>> {
        self.ensure_user_current(user).await?;
        let path = to_normalized(path.as_ref())?;
        let entries = self.list_all(&user.namespace_id, &path).await?;
        let mut items = Vec::with_capacity(entries.len());
        for entry in entries {
            let entry_path = to_normalized(Path::new(&entry.path))?;
            items.push(Fileinfo {
                path: PathBuf::from(to_client_path(&entry_path)),
                metadata: Self::entry_metadata(&entry),
            });
        }
        Ok(items)
    }

    async fn get<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &VfilesFtpUser,
        path: P,
        start_pos: u64,
    ) -> Result<Box<dyn AsyncRead + Send + Sync + Unpin>> {
        self.ensure_user_current(user).await?;
        let path = to_normalized(path.as_ref())?;
        if path.as_str().is_empty() {
            return Err(Error::new(
                ErrorKind::PermanentFileNotAvailable,
                "不能下载目录",
            ));
        }

        let content = self
            .deps
            .workspace
            .open_file(&user.namespace_id, &path, None)
            .await
            .map_err(to_ftp_error)?;

        let mut reader = content.reader;
        if start_pos > 0 {
            use tokio::io::AsyncSeekExt;
            reader
                .seek(std::io::SeekFrom::Start(start_pos))
                .await
                .map_err(|err| transfer_aborted(format!("定位读取位置失败: {err}")))?;
        }

        self.deps
            .stats
            .record_download(content.size_bytes.saturating_sub(start_pos));

        Ok(Box::new(AccountRevalidatingReader::new(
            SyncReader::new(reader),
            Arc::clone(&self.deps.user_repo),
            user.clone(),
        )))
    }

    async fn put<P, R>(
        &self,
        user: &VfilesFtpUser,
        input: R,
        path: P,
        start_pos: u64,
    ) -> Result<u64>
    where
        P: AsRef<Path> + Send + fmt::Debug,
        R: AsyncRead + Send + Sync + Unpin + 'static,
    {
        self.ensure_user_current(user).await?;
        if start_pos > 0 {
            // 不声明上传续传能力：追加语义无法安全映射到「生成新版本」
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "不支持续传上传（APPE/REST），请重新上传完整文件",
            ));
        }

        let path = to_normalized(path.as_ref())?;
        if path.as_str().is_empty() {
            return Err(Error::new(
                ErrorKind::PermanentFileNotAvailable,
                "上传目标不能是目录",
            ));
        }

        // Reject deterministic path conflicts before consuming the data stream into a blob.
        self.validate_upload_path(&user.namespace_id, &path).await?;
        let mut paths = Vec::new();
        let mut current = String::new();
        if !path.as_str().contains('/') {
            paths.push(path.clone());
        } else {
            let (parent, _) = path.as_str().rsplit_once('/').expect("nested path");
            for segment in parent.split('/') {
                if !current.is_empty() {
                    current.push('/');
                }
                current.push_str(segment);
                paths.push(
                    NormalizedPath::new(&current)
                        .map_err(|message| Error::new(ErrorKind::PermissionDenied, message))?,
                );
            }
            paths.push(path.clone());
        }
        let observed_entries = self
            .deps
            .entry_repo
            .find_paths(&user.namespace_id, &paths)
            .await
            .map_err(to_ftp_error)?;
        let entries_by_path = observed_entries
            .into_iter()
            .map(|entry| (entry.path_norm.as_str().to_string(), entry))
            .collect::<HashMap<_, _>>();
        let mutation_paths = paths
            .iter()
            .filter(|path_component| {
                **path_component == path || !entries_by_path.contains_key(path_component.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let path_strings = mutation_paths
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>();
        let locked_paths = self
            .deps
            .lock_repo
            .find_paths_with_active_covering_locks(
                &user.namespace_id,
                &path_strings,
                Self::lock_now_ms(),
            )
            .await
            .map_err(to_ftp_error)?;
        if !locked_paths.is_empty() {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "上传目标或新建父目录受 WebDAV 写锁保护",
            ));
        }

        let mut conditions = Vec::with_capacity(mutation_paths.len());
        for path_component in mutation_paths {
            let observed_entry = entries_by_path.get(path_component.as_str());
            conditions.push(EntryWriteCondition {
                namespace_id: user.namespace_id,
                path: path_component.clone(),
                check_entry_state: true,
                expected_entry_id: observed_entry.map(|entry| entry.id),
                expected_version_id: observed_entry.and_then(|entry| entry.current_version_id),
                expected_lock_tokens: Some(Vec::new()),
                expected_additional_lock_states: None,
            });
        }
        let input =
            AccountRevalidatingReader::new(input, Arc::clone(&self.deps.user_repo), user.clone());
        let bytes = self.import_file(user, &path, input, &conditions).await?;
        debug!(user = %user, path = path.as_str(), bytes, "FTP 上传完成");
        Ok(bytes)
    }

    async fn del<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &VfilesFtpUser,
        path: P,
    ) -> Result<()> {
        self.ensure_user_current(user).await?;
        let path = to_normalized(path.as_ref())?;
        if path.as_str().is_empty() {
            return Err(Error::new(ErrorKind::PermissionDenied, "不能删除根目录"));
        }

        let entry = match self.find_entry(&user.namespace_id, &path).await? {
            Some(entry) if entry.entry_type == EntryKind::File => entry,
            Some(_) => {
                return Err(Error::new(
                    ErrorKind::PermanentFileNotAvailable,
                    format!("不是文件: {}", path.as_str()),
                ));
            }
            None => {
                return Err(Error::new(
                    ErrorKind::PermanentFileNotAvailable,
                    format!("文件不存在: {}", path.as_str()),
                ));
            }
        };

        self.flush_batch().await?;
        let expected_lock_tokens = self
            .expected_unlocked_tokens(&user.namespace_id, &path)
            .await?;
        let condition = EntryWriteCondition {
            namespace_id: user.namespace_id,
            path: path.clone(),
            check_entry_state: true,
            expected_entry_id: Some(entry.id),
            expected_version_id: entry.current_version_id,
            expected_lock_tokens: Some(expected_lock_tokens),
            expected_additional_lock_states: None,
        };
        self.deps
            .workspace
            .delete_entries_with_condition(
                &user.namespace_id,
                std::slice::from_ref(&path),
                Some("FTP 删除文件"),
                &user.id,
                &condition,
            )
            .await
            .map(|_| ())
            .map_err(to_ftp_error)
    }

    async fn rmd<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &VfilesFtpUser,
        path: P,
    ) -> Result<()> {
        self.ensure_user_current(user).await?;
        let path = to_normalized(path.as_ref())?;
        if path.as_str().is_empty() {
            return Err(Error::new(ErrorKind::PermissionDenied, "不能删除根目录"));
        }

        self.flush_batch().await?;
        let entry = self
            .find_entry(&user.namespace_id, &path)
            .await?
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::PermanentDirectoryNotAvailable,
                    format!("目录不存在: {}", path.as_str()),
                )
            })?;
        let expected_lock_tokens = self
            .expected_unlocked_tokens(&user.namespace_id, &path)
            .await?;
        let has_descendant_locks = self
            .deps
            .lock_repo
            .has_active_under_path(&user.namespace_id, path.as_str(), Self::lock_now_ms())
            .await
            .map_err(to_ftp_error)?;
        if has_descendant_locks {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "目录子树中存在 WebDAV 写锁",
            ));
        }
        let condition = EntryWriteCondition {
            namespace_id: user.namespace_id,
            path: path.clone(),
            check_entry_state: true,
            expected_entry_id: Some(entry.id),
            expected_version_id: entry.current_version_id,
            expected_lock_tokens: Some(expected_lock_tokens),
            expected_additional_lock_states: Some(vec![vfiles_domain::EntryLockSnapshot {
                path: path.clone(),
                tokens: Vec::new(),
                include_ancestors: false,
                require_subtree_unlocked: true,
            }]),
        };
        self.deps
            .workspace
            .delete_empty_directory_with_condition(
                &user.namespace_id,
                &path,
                Some("FTP 删除目录"),
                &user.id,
                &condition,
            )
            .await
            .map(|_| ())
            .map_err(|err| match err {
                DomainError::DirectoryNotEmpty => Error::new(
                    ErrorKind::PermanentDirectoryNotEmpty,
                    format!("目录非空: {}", path.as_str()),
                ),
                DomainError::NotFound { .. } => Error::new(
                    ErrorKind::PermanentDirectoryNotAvailable,
                    format!("目录不存在: {}", path.as_str()),
                ),
                DomainError::PathConflict { .. } => Error::new(
                    ErrorKind::PermanentDirectoryNotAvailable,
                    format!("不是目录: {}", path.as_str()),
                ),
                err => to_ftp_error(err),
            })
    }

    async fn mkd<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &VfilesFtpUser,
        path: P,
    ) -> Result<()> {
        self.ensure_user_current(user).await?;
        let path = to_normalized(path.as_ref())?;
        if path.as_str().is_empty() {
            return Err(Error::new(ErrorKind::PermissionDenied, "不能创建根目录"));
        }

        self.flush_batch().await?;
        let mut paths = Vec::new();
        let mut current = String::new();
        for segment in path.as_str().split('/') {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(segment);
            paths.push(
                NormalizedPath::new(&current)
                    .map_err(|message| Error::new(ErrorKind::PermissionDenied, message))?,
            );
        }
        let entries = self
            .deps
            .entry_repo
            .find_paths(&user.namespace_id, &paths)
            .await
            .map_err(to_ftp_error)?;
        let entries_by_path = entries
            .into_iter()
            .map(|entry| (entry.path_norm.as_str().to_string(), entry))
            .collect::<HashMap<_, _>>();
        for ancestor in paths.iter().take(paths.len().saturating_sub(1)) {
            if entries_by_path
                .get(ancestor.as_str())
                .is_some_and(|entry| entry.entry_type != EntryKind::Directory)
            {
                return Err(Error::new(
                    ErrorKind::PermanentDirectoryNotAvailable,
                    format!("不是目录: {}", ancestor.as_str()),
                ));
            }
        }
        if entries_by_path.contains_key(path.as_str()) {
            return Err(Error::new(
                ErrorKind::PermanentDirectoryNotAvailable,
                format!("目录已存在: {}", path.as_str()),
            ));
        }
        let mutation_paths = paths
            .iter()
            .filter(|path_component| {
                **path_component == path || !entries_by_path.contains_key(path_component.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let path_strings = mutation_paths
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>();
        let locked_paths = self
            .deps
            .lock_repo
            .find_paths_with_active_covering_locks(
                &user.namespace_id,
                &path_strings,
                Self::lock_now_ms(),
            )
            .await
            .map_err(to_ftp_error)?;
        if !locked_paths.is_empty() {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "新目录路径受 WebDAV 写锁保护",
            ));
        }

        let mut conditions = Vec::with_capacity(mutation_paths.len());
        for path_component in mutation_paths {
            let observed_entry = entries_by_path.get(path_component.as_str());
            conditions.push(EntryWriteCondition {
                namespace_id: user.namespace_id,
                path: path_component.clone(),
                check_entry_state: true,
                expected_entry_id: observed_entry.map(|entry| entry.id),
                expected_version_id: observed_entry.and_then(|entry| entry.current_version_id),
                expected_lock_tokens: Some(Vec::new()),
                expected_additional_lock_states: None,
            });
        }
        self.deps
            .workspace
            .create_directory_with_conditions(
                &user.namespace_id,
                &path,
                Some("FTP 创建目录"),
                &user.id,
                &conditions,
            )
            .await
            .map_err(to_ftp_error)?;
        debug!(user = %user, path = path.as_str(), "FTP 创建目录");
        Ok(())
    }

    async fn rename<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &VfilesFtpUser,
        from: P,
        to: P,
    ) -> Result<()> {
        self.ensure_user_current(user).await?;
        let from = to_normalized(from.as_ref())?;
        let to = to_normalized(to.as_ref())?;
        if from.as_str().is_empty() || to.as_str().is_empty() {
            return Err(Error::new(ErrorKind::PermissionDenied, "不能重命名根目录"));
        }

        self.flush_batch().await?;
        let source_entry = self
            .find_entry(&user.namespace_id, &from)
            .await?
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::PermanentFileNotAvailable,
                    format!("路径不存在: {}", from.as_str()),
                )
            })?;
        let source_subtree = self.find_rename_subtree(&user.namespace_id, &from).await?;
        let mut destination_paths = Vec::with_capacity(source_subtree.len());
        let mut affected_path_bytes = 0usize;
        for entry in &source_subtree {
            let suffix = &entry.path_norm.as_str()[from.as_str().len()..];
            let destination_path = to_normalized(Path::new(&format!("{}{}", to.as_str(), suffix)))?;
            let next_path_bytes = affected_path_bytes
                .saturating_add(entry.path_norm.as_str().len())
                .saturating_add(destination_path.as_str().len());
            if next_path_bytes > MAX_RENAME_SUBTREE_PATH_BYTES {
                return Err(Error::new(
                    ErrorKind::LocalError,
                    "FTP 重命名目录路径数据超过安全上限",
                ));
            }
            affected_path_bytes = next_path_bytes;
            destination_paths.push(destination_path);
        }
        let lock_now = Self::lock_now_ms();
        let source_has_locks = self
            .deps
            .lock_repo
            .has_active_under_path(&user.namespace_id, from.as_str(), lock_now)
            .await
            .map_err(to_ftp_error)?
            || self
                .deps
                .lock_repo
                .find_active_covering(&user.namespace_id, from.as_str(), lock_now)
                .await
                .map_err(to_ftp_error)?
                .is_some();
        let destination_has_locks = self
            .deps
            .lock_repo
            .has_active_under_path(&user.namespace_id, to.as_str(), lock_now)
            .await
            .map_err(to_ftp_error)?
            || self
                .deps
                .lock_repo
                .find_active_covering(&user.namespace_id, to.as_str(), lock_now)
                .await
                .map_err(to_ftp_error)?
                .is_some();
        if source_has_locks || destination_has_locks {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "源或目标路径受 WebDAV 写锁保护",
            ));
        }
        let mut additional_lock_states = source_subtree
            .iter()
            .filter(|entry| entry.path_norm != from)
            .map(|entry| vfiles_domain::EntryLockSnapshot {
                path: entry.path_norm.clone(),
                tokens: Vec::new(),
                include_ancestors: true,
                require_subtree_unlocked: false,
            })
            .collect::<Vec<_>>();
        additional_lock_states.extend(destination_paths.into_iter().map(|path| {
            vfiles_domain::EntryLockSnapshot {
                path,
                tokens: Vec::new(),
                include_ancestors: true,
                require_subtree_unlocked: false,
            }
        }));
        additional_lock_states.push(vfiles_domain::EntryLockSnapshot {
            path: from.clone(),
            tokens: Vec::new(),
            include_ancestors: false,
            require_subtree_unlocked: true,
        });
        additional_lock_states.push(vfiles_domain::EntryLockSnapshot {
            path: to.clone(),
            tokens: Vec::new(),
            include_ancestors: false,
            require_subtree_unlocked: true,
        });
        let condition = EntryWriteCondition {
            namespace_id: user.namespace_id,
            path: from.clone(),
            check_entry_state: true,
            expected_entry_id: Some(source_entry.id),
            expected_version_id: source_entry.current_version_id,
            expected_lock_tokens: Some(Vec::new()),
            expected_additional_lock_states: Some(additional_lock_states),
        };
        self.deps
            .workspace
            .move_entry_overwriting_with_condition_and_limits(
                &user.namespace_id,
                &from,
                &to,
                Some("FTP 重命名"),
                &user.id,
                vfiles_app::MoveOptions {
                    destination_is_container: false,
                    condition: Some(&condition),
                    ..Default::default()
                },
                MoveSubtreeLimits {
                    max_entries: MAX_RENAME_SUBTREE_ENTRIES,
                    max_path_bytes: MAX_RENAME_SUBTREE_PATH_BYTES,
                },
            )
            .await
            .map_err(to_ftp_error)?;
        Ok(())
    }

    async fn cwd<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &VfilesFtpUser,
        path: P,
    ) -> Result<()> {
        self.ensure_user_current(user).await?;
        let path = to_normalized(path.as_ref())?;
        if path.as_str().is_empty() {
            return Ok(());
        }

        match self.find_entry(&user.namespace_id, &path).await? {
            Some(entry) if entry.entry_type == EntryKind::Directory => Ok(()),
            Some(_) => Err(Error::new(
                ErrorKind::PermanentDirectoryNotAvailable,
                format!("不是目录: {}", path.as_str()),
            )),
            None => Err(Error::new(
                ErrorKind::PermanentDirectoryNotAvailable,
                format!("目录不存在: {}", path.as_str()),
            )),
        }
    }
}

fn tree_item_memory_hint(item: &TreeItem) -> usize {
    let mut bytes = std::mem::size_of::<TreeItem>()
        .saturating_add(item.name.len())
        .saturating_add(item.path.len());
    if let Some(mime_type) = item.mime_type.as_ref() {
        bytes = bytes.saturating_add(mime_type.len());
    }
    if let Some(preview_kind) = item.preview_kind.as_ref() {
        bytes = bytes.saturating_add(preview_kind.len());
    }
    if let Some(last_change) = item.last_change.as_ref() {
        bytes = bytes.saturating_add(last_change.actor_name.len());
        if let Some(message) = last_change.message.as_ref() {
            bytes = bytes.saturating_add(message.len());
        }
    }
    bytes
}

/// 包装领域读取器，使其满足 libunftp 要求的 `Send + Sync`。
///
/// 领域读取器是 `Box<dyn ReadSeek + Send + Unpin>`（不含 `Sync`）；FTP 数据通路
/// 由单会话顺序消费，用 `Mutex` 提供 `Sync` 是安全的：`poll_read` 不跨 await 持锁。
struct SyncReader {
    inner: SyncMutex<Box<dyn vfiles_domain::ReadSeek + Send + Unpin>>,
}

impl fmt::Debug for SyncReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SyncReader")
    }
}

impl SyncReader {
    fn new(inner: Box<dyn vfiles_domain::ReadSeek + Send + Unpin>) -> Self {
        Self {
            inner: SyncMutex::new(inner),
        }
    }
}

impl AsyncRead for SyncReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::pin::Pin::new(&mut **guard).poll_read(cx, buf)
    }
}
