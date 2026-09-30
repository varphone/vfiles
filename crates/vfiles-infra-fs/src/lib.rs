//! Filesystem infrastructure for VFiles.

pub mod store;

use camino::Utf8Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use vfiles_domain::DomainResult;

#[derive(Debug)]
pub struct FsStorageBootstrap;

impl FsStorageBootstrap {
    pub async fn bootstrap(root: &Utf8Path) -> DomainResult<()> {
        let dirs = vec!["blobs", "uploads", "tmp", "export", "logs", "backups"];
        for dir in dirs {
            let path = root.join(dir);
            fs::create_dir_all(&path)
                .await
                .map_err(|e| vfiles_domain::DomainError::Internal {
                    message: format!("Failed to create directory {}: {}", path, e),
                })?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct FsHealthProbe;

impl FsHealthProbe {
    pub async fn check_readiness(root: &Utf8Path) -> DomainResult<()> {
        for relative_path in ["blobs", "uploads", "tmp"] {
            let directory = root.join(relative_path);
            let test_file = directory.join(format!(".vfiles-health-{}.tmp", uuid::Uuid::new_v4()));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&test_file)
                .await
                .map_err(|e| vfiles_domain::DomainError::Internal {
                    message: format!("Storage write check failed for {}: {}", directory, e),
                })?;

            let write_result = file.write_all(b"ready").await;
            drop(file);
            if let Err(error) = write_result {
                let _ = fs::remove_file(&test_file).await;
                return Err(vfiles_domain::DomainError::Internal {
                    message: format!("Storage write check failed for {}: {}", directory, error),
                });
            }

            fs::remove_file(&test_file).await.map_err(|e| {
                vfiles_domain::DomainError::Internal {
                    message: format!("Storage cleanup check failed for {}: {}", directory, e),
                }
            })?;
        }
        Ok(())
    }
}
