//! Local namespace listing and file copy commands.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, anyhow, bail};
use vfiles_app::{ImportBatch, SnapshotMode};
use vfiles_domain::*;
use vfiles_infra_sqlite::{SqliteMigrations, SqlitePoolFactory, repo::*};

struct ContextRepos {
    namespace: NamespaceId,
    user: UserId,
    entries: Arc<dyn EntryRepo + Send + Sync>,
    snapshots: Arc<dyn SnapshotRepo + Send + Sync>,
    blobs: Arc<dyn BlobStore + Send + Sync>,
}

async fn open(owner: Option<String>) -> anyhow::Result<ContextRepos> {
    let config = vfiles_config::ConfigLoader::load()?;
    vfiles_config::ConfigLoader::validate(&config)?;
    let paths = vfiles_config::AppPaths::from_config(&config.storage);
    let pool = SqlitePoolFactory::connect(&paths.database).await?;
    SqliteMigrations::run(&pool).await?;
    let users: Arc<dyn UserRepo + Send + Sync> = Arc::new(SqliteUserRepo::new(pool.clone()));
    let namespaces: Arc<dyn NamespaceRepo + Send + Sync> =
        Arc::new(SqliteNamespaceRepo::new(pool.clone()));
    let entries: Arc<dyn EntryRepo + Send + Sync> = Arc::new(SqliteEntryRepo::new(pool.clone()));
    let snapshots: Arc<dyn SnapshotRepo + Send + Sync> =
        Arc::new(SqliteSnapshotRepo::new(pool.clone()));
    let blobs: Arc<dyn BlobStore + Send + Sync> = Arc::new(FsBlobStore::new(pool, paths.blobs));
    let (user, namespace) = if let Some(owner) = owner {
        let username = Username::new(&owner).map_err(|error| anyhow!("用户名非法: {error}"))?;
        let user = users
            .find_by_username(&username)
            .await
            .context("用户不存在")?;
        let namespace = namespaces.find_default_for_owner(&user.id).await?;
        (user.id, namespace)
    } else {
        let (user, _) =
            crate::import_cmd::resolve_default_owner(users.as_ref(), namespaces.as_ref()).await?;
        let namespace = namespaces.find_default_for_owner(&user).await?;
        (user, namespace)
    };
    Ok(ContextRepos {
        namespace,
        user,
        entries,
        snapshots,
        blobs,
    })
}

fn path(raw: &str) -> anyhow::Result<NormalizedPath> {
    NormalizedPath::new(raw.trim_matches('/')).map_err(|error| anyhow!("路径非法: {error}"))
}

pub async fn run_ls(raw: Option<String>, owner: Option<String>) -> anyhow::Result<()> {
    let repos = open(owner).await?;
    let parent = path(raw.as_deref().unwrap_or(""))?;
    let children = repos
        .entries
        .find_children(&repos.namespace, &parent)
        .await?;
    for entry in children {
        let marker = if entry.entry_type == EntryKind::Directory {
            "/"
        } else {
            ""
        };
        println!("{}{marker}", entry.name);
    }
    Ok(())
}

pub async fn run_ci(source: PathBuf, target: String, owner: Option<String>) -> anyhow::Result<()> {
    let metadata = tokio::fs::symlink_metadata(&source)
        .await
        .with_context(|| format!("无法读取本地文件: {}", source.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("ci 目前只接受普通本地文件: {}", source.display());
    }
    let target = path(&target)?;
    if target.as_str().is_empty() {
        bail!("目标路径必须包含文件名");
    }
    let repos = open(owner).await?;
    let mut batch = ImportBatch::with_options(
        Arc::clone(&repos.entries),
        Arc::clone(&repos.snapshots),
        Arc::clone(&repos.blobs),
        repos.namespace,
        repos.user,
        "CLI 文件复制",
        SnapshotMode::PerFile,
        1,
        true,
    );
    let file = tokio::fs::File::open(&source).await?;
    batch
        .import_file_stream(&target, Box::new(file), None, Some("CLI 文件复制"))
        .await
        .with_context(|| format!("写入用户空间路径失败: {}", target.as_str()))?;
    batch.finish().await?;
    Ok(())
}

pub async fn run_co(raw: String, target: PathBuf, owner: Option<String>) -> anyhow::Result<()> {
    let source = path(&raw)?;
    let repos = open(owner).await?;
    let root = repos
        .entries
        .find_by_path(&repos.namespace, &source)
        .await?
        .ok_or_else(|| anyhow!("用户空间路径不存在: {}", source.as_str()))?;
    if root.entry_type == EntryKind::File {
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await?;
        write_file(&repos, &root, &target).await?;
        return Ok(());
    }

    tokio::fs::create_dir_all(&target).await?;
    let entries = repos
        .entries
        .find_subtree(&repos.namespace, &source)
        .await?;
    for entry in entries
        .into_iter()
        .filter(|entry| entry.path_norm != source)
    {
        let relative = entry
            .path_norm
            .as_str()
            .strip_prefix(source.as_str())
            .and_then(|suffix| suffix.strip_prefix('/'))
            .ok_or_else(|| anyhow!("存储路径不在源目录下: {}", entry.path_norm.as_str()))?;
        let local = target.join(relative);
        if entry.entry_type == EntryKind::Directory {
            tokio::fs::create_dir_all(local).await?;
        } else {
            if let Some(parent) = local.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            write_file(&repos, &entry, &local).await?;
        }
    }
    Ok(())
}

async fn write_file(repos: &ContextRepos, entry: &Entry, target: &Path) -> anyhow::Result<()> {
    let version_id = entry
        .current_version_id
        .ok_or_else(|| anyhow!("文件没有当前版本: {}", entry.path_norm.as_str()))?;
    let version = repos.entries.find_version(&version_id).await?;
    let blob_id = version
        .blob_id
        .ok_or_else(|| anyhow!("文件版本没有 blob: {}", entry.path_norm.as_str()))?;
    let bytes = repos
        .blobs
        .get_blob(&blob_id)
        .await?
        .ok_or_else(|| anyhow!("文件内容缺失: {}", entry.path_norm.as_str()))?;
    tokio::fs::write(target, bytes)
        .await
        .with_context(|| format!("写入本地文件失败: {}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::path;

    #[test]
    fn namespace_paths_allow_root_and_strip_outer_slashes() {
        assert_eq!(path("/").unwrap().as_str(), "");
        assert_eq!(
            path("/docs/report.txt/").unwrap().as_str(),
            "docs/report.txt"
        );
        assert!(path("../outside").is_err());
    }
}
