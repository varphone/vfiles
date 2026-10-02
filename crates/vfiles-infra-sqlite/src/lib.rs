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
use std::time::Duration;
use std::{collections::HashSet, str::FromStr};
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

/// Create or remove the optional filename trigram index as an atomic unit.
///
/// The index is deliberately opt-in because it materially increases path-write
/// cost and database size. Enabling it backfills existing paths before install;
/// future entry inserts, deletes, and path updates are maintained by triggers.
pub async fn configure_filename_trigram_index(
    pool: &SqlitePool,
    enabled: bool,
) -> DomainResult<()> {
    const TABLE: &str = "entries_path_trigram";
    const TRIGGERS: [&str; 3] = [
        "entries_path_trigram_insert",
        "entries_path_trigram_delete",
        "entries_path_trigram_update",
    ];
    const DROP_TRIGGER_SQL: [&str; 3] = [
        "DROP TRIGGER IF EXISTS entries_path_trigram_insert",
        "DROP TRIGGER IF EXISTS entries_path_trigram_delete",
        "DROP TRIGGER IF EXISTS entries_path_trigram_update",
    ];

    let mut transaction =
        pool.begin()
            .await
            .map_err(|error| vfiles_domain::DomainError::Internal {
                message: format!("Failed to configure filename trigram index: {error}"),
            })?;
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
    )
    .bind(TABLE)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| vfiles_domain::DomainError::Internal {
        message: format!("Failed to inspect filename trigram index: {error}"),
    })?;

    let trigger_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name IN (?, ?, ?)",
    )
    .bind(TRIGGERS[0])
    .bind(TRIGGERS[1])
    .bind(TRIGGERS[2])
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| vfiles_domain::DomainError::Internal {
        message: format!("Failed to inspect filename trigram maintenance: {error}"),
    })?;

    if enabled && (!table_exists || trigger_count != TRIGGERS.len() as i64) {
        for query in DROP_TRIGGER_SQL {
            sqlx::query(query)
                .execute(&mut *transaction)
                .await
                .map_err(|error| vfiles_domain::DomainError::Internal {
                    message: format!("Failed to repair filename trigram maintenance: {error}"),
                })?;
        }
        sqlx::query("DROP TABLE IF EXISTS entries_path_trigram")
            .execute(&mut *transaction)
            .await
            .map_err(|error| vfiles_domain::DomainError::Internal {
                message: format!("Failed to reset filename trigram index: {error}"),
            })?;
        sqlx::query(
            "CREATE VIRTUAL TABLE entries_path_trigram USING fts5(\
                path_lower, content = '', tokenize = 'trigram case_sensitive 1', detail = none\
            )",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| vfiles_domain::DomainError::Internal {
            message: format!("Failed to create filename trigram index: {error}"),
        })?;
        sqlx::query(
            "INSERT INTO entries_path_trigram(rowid, path_lower) \
             SELECT rowid, lower(path) FROM entries",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| vfiles_domain::DomainError::Internal {
            message: format!("Failed to backfill filename trigram index: {error}"),
        })?;
        sqlx::query(
            "CREATE TRIGGER entries_path_trigram_insert AFTER INSERT ON entries BEGIN \
             INSERT INTO entries_path_trigram(rowid, path_lower) VALUES(new.rowid, lower(new.path)); \
             END",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| vfiles_domain::DomainError::Internal {
            message: format!("Failed to create filename trigram insert trigger: {error}"),
        })?;
        sqlx::query(
            "CREATE TRIGGER entries_path_trigram_delete AFTER DELETE ON entries BEGIN \
             INSERT INTO entries_path_trigram(entries_path_trigram, rowid, path_lower) \
             VALUES('delete', old.rowid, lower(old.path)); END",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| vfiles_domain::DomainError::Internal {
            message: format!("Failed to create filename trigram delete trigger: {error}"),
        })?;
        sqlx::query(
            "CREATE TRIGGER entries_path_trigram_update AFTER UPDATE OF path ON entries BEGIN \
             INSERT INTO entries_path_trigram(entries_path_trigram, rowid, path_lower) \
             VALUES('delete', old.rowid, lower(old.path)); \
             INSERT INTO entries_path_trigram(rowid, path_lower) VALUES(new.rowid, lower(new.path)); \
             END",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| vfiles_domain::DomainError::Internal {
            message: format!("Failed to create filename trigram update trigger: {error}"),
        })?;
    } else if !enabled && (table_exists || trigger_count != 0) {
        for query in DROP_TRIGGER_SQL {
            sqlx::query(query)
                .execute(&mut *transaction)
                .await
                .map_err(|error| vfiles_domain::DomainError::Internal {
                    message: format!("Failed to remove filename trigram maintenance: {error}"),
                })?;
        }
        sqlx::query("DROP TABLE IF EXISTS entries_path_trigram")
            .execute(&mut *transaction)
            .await
            .map_err(|error| vfiles_domain::DomainError::Internal {
                message: format!("Failed to remove filename trigram index: {error}"),
            })?;
    }

    transaction
        .commit()
        .await
        .map_err(|error| vfiles_domain::DomainError::Internal {
            message: format!("Failed to commit filename trigram index setup: {error}"),
        })
}

impl SqliteMigrations {
    pub async fn run(pool: &SqlitePool) -> DomainResult<()> {
        let mut migrator = sqlx::migrate!("./migrations");
        let known_versions: HashSet<_> =
            migrator.iter().map(|migration| migration.version).collect();
        let migration_table_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
        )
        .fetch_one(pool)
        .await
        .map_err(|error| vfiles_domain::DomainError::Internal {
            message: format!("Failed to inspect database migrations: {error}"),
        })?;

        if migration_table_exists != 0 {
            let applied_migrations: Vec<(i64, String)> = sqlx::query_as(
                "SELECT version, description FROM _sqlx_migrations WHERE success = 1",
            )
            .fetch_all(pool)
            .await
            .map_err(|error| vfiles_domain::DomainError::Internal {
                message: format!("Failed to inspect applied migrations: {error}"),
            })?;
            let missing_migrations: Vec<_> = applied_migrations
                .into_iter()
                .filter(|(version, _)| !known_versions.contains(version))
                .collect();
            if missing_migrations
                .iter()
                .any(|(version, description)| *version != 23 || description != "share_code_entropy")
            {
                return Err(vfiles_domain::DomainError::Internal {
                    message: format!(
                        "Applied migrations are missing from the current build: {missing_migrations:?}"
                    ),
                });
            }
            if !missing_migrations.is_empty() {
                // Version 23 existed briefly and replaced user-facing short codes with long UUIDs.
                migrator.set_ignore_missing(true);
            }
        }

        migrator
            .run(pool)
            .await
            .map_err(|e| vfiles_domain::DomainError::Internal {
                message: format!("Migration failed: {}", e),
            })?;

        backfill_share_public_codes(pool).await?;

        Ok(())
    }
}

async fn backfill_share_public_codes(pool: &SqlitePool) -> DomainResult<()> {
    let mut transaction =
        pool.begin()
            .await
            .map_err(|error| vfiles_domain::DomainError::Internal {
                message: format!("Failed to start share-code backfill: {error}"),
            })?;
    let missing_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM shares WHERE public_code IS NULL ORDER BY created_at, id",
    )
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| vfiles_domain::DomainError::Internal {
        message: format!("Failed to list shares without public codes: {error}"),
    })?;

    for share_id in missing_ids {
        loop {
            let public_code = vfiles_domain::generate_short_share_code();
            let result = sqlx::query(
                "UPDATE shares SET public_code = ? WHERE id = ? AND public_code IS NULL",
            )
            .bind(public_code)
            .bind(&share_id)
            .execute(&mut *transaction)
            .await;

            match result {
                Ok(_) => break,
                Err(error)
                    if error
                        .as_database_error()
                        .is_some_and(|database_error| database_error.is_unique_violation()) =>
                {
                    continue;
                }
                Err(error) => {
                    return Err(vfiles_domain::DomainError::Internal {
                        message: format!("Failed to assign a short share code: {error}"),
                    });
                }
            }
        }
    }

    transaction
        .commit()
        .await
        .map_err(|error| vfiles_domain::DomainError::Internal {
            message: format!("Failed to commit share-code backfill: {error}"),
        })
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
