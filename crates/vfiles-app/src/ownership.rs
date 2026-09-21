//! 所有权转移：把自己的文件/目录交给另一个用户。
//!
//! 实现要点：把条目的 `namespace_id` 改为目标用户的命名空间即可——
//! 版本历史（`entry_versions`）按条目 ID 关联，会**随条目一起转移**；
//! blob 是全局内容寻址存储，不需要复制。
//!
//! 同时在源与目标命名空间各写一条快照，让双方的版本时间线都能看到这次变化。

use std::collections::HashSet;
use std::sync::Arc;

use vfiles_domain::{
    DomainError, DomainResult, EntryRepo, NamespaceId, NamespaceRepo, NormalizedPath, SnapshotKind,
    SnapshotRepo, UserId, UserRepo,
};

use crate::services::{create_snapshot_record, ensure_directory_path};
use crate::{NamespaceService, collect_snapshot_state};

/// 转移结果：实际转移的条目数与目标用户名（用于提示与审计）。
#[derive(Debug, Clone)]
pub struct TransferOutcome {
    pub transferred: usize,
    pub target_username: String,
}

/// `docs/a/b.txt` → ["docs", "docs/a"]（由浅到深，不含空串）。
fn ancestors_of(path: &str) -> Vec<String> {
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let mut ancestors = Vec::new();
    let mut current = String::new();
    for segment in segments.iter().take(segments.len().saturating_sub(1)) {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        ancestors.push(current.clone());
    }
    ancestors
}

/// 所有权转移服务。
///
/// 依赖用 `Arc<dyn ...>` 持有：既能直接复用 [`crate::AppState`] 里已有的仓储实例，
/// 又不需要给服务加一堆泛型参数。
#[derive(Clone)]
pub struct OwnershipService {
    entry_repo: Arc<dyn EntryRepo + Send + Sync>,
    namespace_repo: Arc<dyn NamespaceRepo + Send + Sync>,
    user_repo: Arc<dyn UserRepo + Send + Sync>,
    snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
}

impl OwnershipService {
    pub fn new(
        entry_repo: Arc<dyn EntryRepo + Send + Sync>,
        namespace_repo: Arc<dyn NamespaceRepo + Send + Sync>,
        user_repo: Arc<dyn UserRepo + Send + Sync>,
        snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
    ) -> Self {
        Self {
            entry_repo,
            namespace_repo,
            user_repo,
            snapshot_repo,
        }
    }

    /// 可用作转移目标的其他用户（启用中，排除自己）。
    pub async fn list_targets(&self, actor: &UserId) -> DomainResult<Vec<(UserId, String)>> {
        let users = self.user_repo.list_transfer_targets(actor).await?;
        Ok(users
            .into_iter()
            .map(|user| (user.id, user.username.to_string()))
            .collect())
    }

    /// 把 `source_namespace` 下的若干路径（含目录子树）转给 `target_user_id`。
    ///
    /// - 目标必须存在、未禁用，且不能是自己；
    /// - 同名路径在目标下已存在时整体失败（避免静默覆盖）；
    /// - 版本历史随条目一起转移，双方各留一条快照。
    pub async fn transfer(
        &self,
        source_namespace: &NamespaceId,
        paths: &[NormalizedPath],
        target_user_id: &UserId,
        actor_user_id: &UserId,
        actor_username: &str,
        message: Option<&str>,
    ) -> DomainResult<TransferOutcome> {
        if paths.is_empty() {
            return Err(DomainError::Validation {
                message: "At least one path is required".to_string(),
            });
        }

        if paths.iter().any(|path| path.as_str().is_empty()) {
            return Err(DomainError::Validation {
                message: "The root directory cannot be transferred".to_string(),
            });
        }

        // 源路径之间不允许重叠（避免同一子树被转移两次）
        for (index, source) in paths.iter().enumerate() {
            for other in paths.iter().skip(index + 1) {
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

        if target_user_id == actor_user_id {
            return Err(DomainError::Validation {
                message: "Cannot transfer ownership to yourself".to_string(),
            });
        }

        let target_user = self.user_repo.find_by_id(target_user_id).await?;
        if target_user.disabled {
            return Err(DomainError::Validation {
                message: "Target user is disabled".to_string(),
            });
        }

        let namespace_service = NamespaceService::new(self.namespace_repo.clone());
        let target_namespace = namespace_service
            .ensure_default_for_owner(target_user_id)
            .await?;

        if &target_namespace == source_namespace {
            return Err(DomainError::Validation {
                message: "Target user shares the same namespace".to_string(),
            });
        }

        // 收集待转移条目（目录含整棵子树）并检查目标侧同名路径
        let mut moves = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        // 转移后需要在目标命名空间补齐的祖先目录（不含本次一起转移的目录）
        let mut needed_parents: HashSet<String> = HashSet::new();
        let mut transferred_paths: HashSet<String> = HashSet::new();
        for path in paths {
            let subtree = self.entry_repo.find_subtree(source_namespace, path).await?;
            if subtree.is_empty() {
                return Err(DomainError::NotFound {
                    resource: format!("entry {}", path.as_str()),
                });
            }

            for entry in subtree {
                if !seen.insert(entry.path_norm.as_str().to_string()) {
                    continue;
                }
                transferred_paths.insert(entry.path_norm.as_str().to_string());
                if self
                    .entry_repo
                    .find_by_path(&target_namespace, &entry.path_norm)
                    .await?
                    .is_some()
                {
                    return Err(DomainError::Conflict {
                        message: format!(
                            "Target user already has an entry at {}",
                            entry.path_norm.as_str()
                        ),
                    });
                }
                // 记录祖先目录（稍后在目标命名空间补齐）
                for ancestor in ancestors_of(entry.path_norm.as_str()) {
                    needed_parents.insert(ancestor);
                }
                moves.push((entry.id, target_namespace));
            }
        }

        if moves.is_empty() {
            return Err(DomainError::NotFound {
                resource: "entry".to_string(),
            });
        }

        // 目标命名空间里可能没有对应的父目录（例如只转一个 docs/a.txt）：
        // 先补齐**不属于本次转移集合**的祖先目录，保证转移后条目落在同样的路径下。
        let mut parents: Vec<&String> = needed_parents
            .iter()
            .filter(|path| !transferred_paths.contains(*path))
            .collect();
        parents.sort();
        for path in parents {
            let parent = NormalizedPath::new(path).map_err(|_| DomainError::Validation {
                message: format!("Invalid path format: {path}"),
            })?;
            ensure_directory_path(
                self.entry_repo.as_ref(),
                &target_namespace,
                &parent,
                actor_user_id,
            )
            .await?;
        }

        let transferred = moves.len();
        self.entry_repo.transfer_entries(&moves).await?;

        // 双方各写一条快照：源侧记录「消失」，目标侧记录「出现」
        let note = message
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let source_message = match &note {
            Some(text) => format!("转移给 {}：{text}", target_user.username),
            None => format!("转移给 {}", target_user.username),
        };
        let target_message = match &note {
            Some(text) => format!("接收来自 {actor_username} 的文件：{text}"),
            None => format!("接收来自 {actor_username} 的文件"),
        };

        let source_entries =
            collect_snapshot_state(self.entry_repo.as_ref(), source_namespace, Vec::new()).await?;
        create_snapshot_record(
            self.snapshot_repo.as_ref(),
            source_namespace,
            Some(&source_message),
            SnapshotKind::UserCreated,
            actor_user_id,
            source_entries,
        )
        .await?;

        let target_entries =
            collect_snapshot_state(self.entry_repo.as_ref(), &target_namespace, Vec::new()).await?;
        create_snapshot_record(
            self.snapshot_repo.as_ref(),
            &target_namespace,
            Some(&target_message),
            SnapshotKind::UserCreated,
            actor_user_id,
            target_entries,
        )
        .await?;

        Ok(TransferOutcome {
            transferred,
            target_username: target_user.username.to_string(),
        })
    }
}
