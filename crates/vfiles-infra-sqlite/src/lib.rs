//! SQLite infrastructure for VFiles.

pub mod repo;

pub use repo::{
    FsBlobStore, FsUploadStore, SqliteAccessTokenRepo, SqliteAdminRepo, SqliteAuditLogRepo,
    SqliteEntryRepo, SqliteFavoriteRepo, SqliteNamespaceRepo, SqliteS3DeleteMarkerRepo,
    SqliteS3MultipartUploadRepo, SqliteS3ObjectKeyRepo, SqliteSearchRepo, SqliteSessionRepo,
    SqliteShareRepo, SqliteSnapshotRepo, SqliteSystemSettingsRepo, SqliteUserRepo,
    SqliteWebdavLockRepo,
};

use camino::Utf8Path;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Pool, Sqlite};
use std::str::FromStr;
use std::time::Duration;
use vfiles_domain::DomainResult;

pub type SqlitePool = Pool<Sqlite>;

#[derive(Debug)]
pub struct SqlitePoolFactory;

impl SqlitePoolFactory {
    pub async fn connect(database_path: &Utf8Path) -> DomainResult<SqlitePool> {
        // Ensure the parent directory exists
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| vfiles_domain::DomainError::Internal {
                message: format!("Failed to create database directory {}: {}", parent, e),
            })?;
        }

        // Convert to absolute path
        let abs_path: std::path::PathBuf = if database_path.is_absolute() {
            database_path.into()
        } else {
            std::env::current_dir()
                .map_err(|e| vfiles_domain::DomainError::Internal {
                    message: format!("Failed to get current directory: {}", e),
                })?
                .join(database_path)
        };

        let abs_path_utf8 = camino::Utf8PathBuf::from_path_buf(abs_path).map_err(|_| {
            vfiles_domain::DomainError::Internal {
                message: "Database path is not valid UTF-8".to_string(),
            }
        })?;

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", abs_path_utf8))
            .map_err(|e| vfiles_domain::DomainError::Internal {
                message: format!("Failed to build database connection options: {}", e),
            })?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));

        SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .map_err(|e| vfiles_domain::DomainError::Internal {
                message: format!("Failed to connect to database: {}", e),
            })
    }
}

#[derive(Debug)]
pub struct SqliteMigrations;

impl SqliteMigrations {
    pub async fn run(pool: &SqlitePool) -> DomainResult<()> {
        sqlx::migrate!("./migrations")
            .run(pool)
            .await
            .map_err(|e| vfiles_domain::DomainError::Internal {
                message: format!("Migration failed: {}", e),
            })?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct SqliteHealthProbe;

impl SqliteHealthProbe {
    pub async fn check_readiness(pool: &SqlitePool) -> DomainResult<()> {
        sqlx::query("SELECT 1").execute(pool).await.map_err(|e| {
            vfiles_domain::DomainError::Internal {
                message: format!("Database health check failed: {}", e),
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[tokio::test]
    async fn share_code_migration_rotates_short_codes() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database should connect");
        sqlx::query("CREATE TABLE shares (id TEXT PRIMARY KEY, code TEXT NOT NULL UNIQUE)")
            .execute(&pool)
            .await
            .expect("shares table should be created");
        sqlx::query("INSERT INTO shares (id, code) VALUES ('legacy-a', 'deadbeef'), ('legacy-b', '12345678'), ('current', ?)")
            .bind("a".repeat(32))
            .execute(&pool)
            .await
            .expect("test shares should be inserted");

        sqlx::raw_sql(include_str!("../migrations/0023_share_code_entropy.sql"))
            .execute(&pool)
            .await
            .expect("share code migration should run");

        let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, code FROM shares ORDER BY id")
            .fetch_all(&pool)
            .await
            .expect("migrated shares should be readable");
        assert_eq!(rows[0].0, "current");
        assert_eq!(rows[0].1, "a".repeat(32));
        for (id, code) in rows.iter().filter(|(id, _)| id.starts_with("legacy-")) {
            assert_eq!(code.len(), 32, "{id} should have a full length code");
            assert!(code.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert_ne!(code.as_str(), "deadbeef");
            assert_ne!(code.as_str(), "12345678");
        }
    }
}
