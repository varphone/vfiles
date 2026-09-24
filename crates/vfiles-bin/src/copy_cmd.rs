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
    max_file_size_bytes: u64,
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
        max_file_size_bytes: config.limits.max_file_size_bytes,
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
    let target = path(&target)?;
    let repos = open(owner).await?;
    copy_local_into_namespace(&repos, &source, &target).await
}

async fn copy_local_into_namespace(
    repos: &ContextRepos,
    source: &Path,
    target: &NormalizedPath,
) -> anyhow::Result<()> {
    let metadata = tokio::fs::symlink_metadata(source)
        .await
        .with_context(|| format!("无法读取本地文件: {}", source.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("ci 不接受符号链接: {}", source.display());
    }
    let source = source
        .canonicalize()
        .with_context(|| format!("无法解析本地路径: {}", source.display()))?;
    let is_directory = metadata.is_dir();
    if !is_directory && !metadata.is_file() {
        bail!("ci 只接受普通文件或目录: {}", source.display());
    }
    if !is_directory && target.as_str().is_empty() {
        bail!("文件目标路径必须包含文件名");
    }
    let plan = if is_directory {
        crate::import_cmd::collect_plan(&source, true)?
    } else {
        crate::import_cmd::ImportPlan {
            files: vec![crate::import_cmd::PlannedFile {
                relative: String::new(),
                absolute: source.clone(),
            }],
            directories: Vec::new(),
            bytes: metadata.len(),
            skipped_symlinks: 0,
        }
    };
    let mut batch = ImportBatch::with_options(
        Arc::clone(&repos.entries),
        Arc::clone(&repos.snapshots),
        Arc::clone(&repos.blobs),
        repos.namespace,
        repos.user,
        "CLI 文件复制",
        if is_directory {
            SnapshotMode::Batch
        } else {
            SnapshotMode::PerFile
        },
        200,
        true,
    );
    let mut errors = Vec::new();
    if is_directory
        && !target.as_str().is_empty()
        && let Err(error) = batch.create_directory(target).await
    {
        errors.push(format!(
            "创建用户空间目录失败: {}: {error}",
            target.as_str()
        ));
    }
    for directory in &plan.directories {
        match crate::import_cmd::join_target(target, &directory.relative) {
            Ok(remote) => {
                if let Err(error) = batch.create_directory(&remote).await {
                    errors.push(format!(
                        "创建用户空间目录失败: {}: {error}",
                        remote.as_str()
                    ));
                }
            }
            Err(error) => errors.push(error.to_string()),
        }
    }
    for file in &plan.files {
        let remote = if is_directory {
            crate::import_cmd::join_target(target, &file.relative)
        } else {
            Ok(target.clone())
        };
        let remote = match remote {
            Ok(remote) => remote,
            Err(error) => {
                errors.push(error.to_string());
                continue;
            }
        };
        let handle = match tokio::fs::File::open(&file.absolute).await {
            Ok(handle) => handle,
            Err(error) => {
                errors.push(format!(
                    "读取本地文件失败: {}: {error}",
                    file.absolute.display()
                ));
                continue;
            }
        };
        if let Err(error) = batch
            .import_file_stream(
                &remote,
                Box::new(handle),
                Some(repos.max_file_size_bytes),
                Some("CLI 文件复制"),
            )
            .await
        {
            errors.push(format!(
                "写入用户空间路径失败: {}: {error}",
                remote.as_str()
            ));
        }
    }
    if let Err(error) = batch.finish().await {
        errors.push(format!("提交用户空间快照失败: {error}"));
    }
    if !errors.is_empty() {
        bail!(
            "复制过程中有 {} 项失败：{}",
            errors.len(),
            errors.join("; ")
        );
    }
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
    use super::{ContextRepos, copy_local_into_namespace, path};
    use std::sync::Arc;

    use camino::Utf8PathBuf;
    use vfiles_domain::{BlobStore, EntryRepo, NamespaceRepo, SnapshotRepo, UserRepo};
    use vfiles_infra_sqlite::{SqliteMigrations, SqlitePoolFactory, repo::*};

    #[test]
    fn namespace_paths_allow_root_and_strip_outer_slashes() {
        assert_eq!(path("/").unwrap().as_str(), "");
        assert_eq!(
            path("/docs/report.txt/").unwrap().as_str(),
            "docs/report.txt"
        );
        assert!(path("../outside").is_err());
    }

    #[tokio::test]
    async fn ci_copies_a_file_or_directory_tree_and_keeps_empty_directories() {
        let temp = tempfile::tempdir().expect("temporary workspace");
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8 path");
        let pool = SqlitePoolFactory::connect(root.join("vfiles.db").as_path())
            .await
            .expect("database connection");
        SqliteMigrations::run(&pool).await.expect("migrations");
        let user_repo = SqliteUserRepo::new(pool.clone());
        let user_id = user_repo
            .create_admin("copytest", "copytest@example.invalid", "hash")
            .await
            .expect("create user");
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let namespace = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("create namespace");
        let entries: Arc<dyn EntryRepo + Send + Sync> =
            Arc::new(SqliteEntryRepo::new(pool.clone()));
        let snapshots: Arc<dyn SnapshotRepo + Send + Sync> =
            Arc::new(SqliteSnapshotRepo::new(pool.clone()));
        let blobs: Arc<dyn BlobStore + Send + Sync> =
            Arc::new(FsBlobStore::new(pool.clone(), root.join("blobs")));
        let repos = ContextRepos {
            namespace,
            user: user_id,
            entries: Arc::clone(&entries),
            snapshots,
            blobs: Arc::clone(&blobs),
            max_file_size_bytes: 1024 * 1024,
        };

        let source_dir = root.join("source");
        std::fs::create_dir_all(source_dir.join("nested/empty")).expect("source directories");
        std::fs::write(source_dir.join("nested/data.txt"), b"directory payload")
            .expect("source file");
        let dir_target = path("tree").expect("namespace target");
        copy_local_into_namespace(&repos, source_dir.as_std_path(), &dir_target)
            .await
            .expect("copy directory");

        let single = root.join("single.txt");
        std::fs::write(&single, b"single payload").expect("single source file");
        let file_target = path("single.txt").expect("file target");
        copy_local_into_namespace(&repos, single.as_std_path(), &file_target)
            .await
            .expect("copy single file");

        let mut paths: Vec<_> = entries
            .find_all(&namespace)
            .await
            .expect("list copied entries")
            .into_iter()
            .map(|entry| entry.path_norm.as_str().to_owned())
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            [
                "single.txt",
                "tree",
                "tree/nested",
                "tree/nested/data.txt",
                "tree/nested/empty"
            ]
        );
        let entry = entries
            .find_by_path(&namespace, &path("tree/nested/data.txt").unwrap())
            .await
            .expect("query file")
            .expect("copied file exists");
        let version = entries
            .find_version(&entry.current_version_id.expect("current version"))
            .await
            .expect("file version");
        let bytes = blobs
            .get_blob(&version.blob_id.expect("file blob"))
            .await
            .expect("read blob")
            .expect("blob exists");
        assert_eq!(bytes, b"directory payload");
    }
}
