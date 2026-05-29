//! Filesystem infrastructure for VFiles.

pub mod store;

use camino::Utf8Path;
use tokio::fs;
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
        // Check if directories exist and are writable
        let test_file = root.join("tmp").join("health_check.tmp");
        fs::write(&test_file, b"test")
            .await
            .map_err(|e| vfiles_domain::DomainError::Internal {
                message: format!("Storage write check failed: {}", e),
            })?;
        fs::remove_file(&test_file)
            .await
            .map_err(|e| vfiles_domain::DomainError::Internal {
                message: format!("Storage cleanup check failed: {}", e),
            })?;
        Ok(())
    }
}
