use sqlx::{Pool, Sqlite};
use vfiles_domain::*;

pub type SqlitePool = Pool<Sqlite>;

fn role_as_str(role: Role) -> &'static str {
    match role {
        Role::Admin => "admin",
        Role::Manager => "manager",
        Role::User => "user",
    }
}

fn parse_role(value: &str) -> DomainResult<Role> {
    match value {
        "admin" => Ok(Role::Admin),
        "manager" => Ok(Role::Manager),
        "user" => Ok(Role::User),
        _ => Err(DomainError::Internal {
            message: "Invalid role".to_string(),
        }),
    }
}

type UserRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    bool,
    String,
    String,
    Option<String>,
);

fn parse_optional_email(value: Option<String>) -> DomainResult<Option<EmailAddress>> {
    value
        .map(|email| {
            EmailAddress::new(&email).map_err(|_| DomainError::Internal {
                message: "Invalid email".to_string(),
            })
        })
        .transpose()
}

fn user_from_row(row: UserRow) -> DomainResult<User> {
    let (
        id,
        username,
        email,
        password_hash,
        role,
        disabled,
        created_at,
        updated_at,
        password_changed_at,
    ) = row;
    let role = parse_role(&role)?;

    Ok(User {
        id: UserId::from_uuid(
            uuid::Uuid::parse_str(&id).map_err(|_| DomainError::Internal {
                message: "Invalid UUID".to_string(),
            })?,
        ),
        username: Username::new(&username).map_err(|_| DomainError::Internal {
            message: "Invalid username".to_string(),
        })?,
        email: parse_optional_email(email)?,
        password_hash,
        role,
        disabled,
        created_at: parse_timestamp(&created_at)?,
        updated_at: parse_timestamp(&updated_at)?,
        password_changed_at: parse_timestamp_opt(password_changed_at.as_deref())?,
    })
}

// Minimal repos for bootstrap
#[derive(Debug)]
pub struct SqliteUserRepo {
    pool: SqlitePool,
}

impl SqliteUserRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl Clone for SqliteUserRepo {
    fn clone(&self) -> Self {
        // For HTTP layer, we need Clone. Use Arc<SqlitePool> in production
        Self {
            pool: self.pool.clone(),
        }
    }
}

#[async_trait::async_trait]
impl UserRepo for SqliteUserRepo {
    async fn create_admin(
        &self,
        username: &str,
        email: &str,
        password_hash: &str,
    ) -> DomainResult<UserId> {
        let id = UserId::new();
        let now = time::OffsetDateTime::now_utc();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role, disabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(username)
        .bind(email)
        .bind(password_hash)
        .bind("admin")
        .bind(false)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to create admin: {}", e),
        })?;
        Ok(id)
    }

    async fn count_admins(&self) -> DomainResult<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = 'admin'")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to count admins: {}", e),
            })?;
        Ok(count)
    }

    async fn create_user(
        &self,
        username: &Username,
        email: Option<&EmailAddress>,
        password_hash: &str,
        role: Role,
    ) -> DomainResult<UserId> {
        let id = UserId::new();
        let now = time::OffsetDateTime::now_utc();
        let role_str = role_as_str(role);

        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role, disabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(username.as_str())
        .bind(email.map(|value| value.as_str()))
        .bind(password_hash)
        .bind(role_str)
        .bind(false)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to create user: {}", e),
        })?;
        Ok(id)
    }

    async fn find_by_username(&self, username: &Username) -> DomainResult<User> {
        let row: UserRow = sqlx::query_as(
            "SELECT id, username, email, password_hash, role, disabled, created_at, updated_at, password_changed_at FROM users WHERE username = ?"
        )
        .bind(username.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::NotFound { resource: "user".to_string() },
            _ => DomainError::Internal { message: format!("Failed to find user: {}", e) },
        })?;

        user_from_row(row)
    }

    async fn find_by_email(&self, email: &EmailAddress) -> DomainResult<User> {
        let row: UserRow = sqlx::query_as(
            "SELECT id, username, email, password_hash, role, disabled, created_at, updated_at, password_changed_at FROM users WHERE email = ?"
        )
        .bind(email.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::NotFound { resource: "user".to_string() },
            _ => DomainError::Internal { message: format!("Failed to find user: {}", e) },
        })?;

        user_from_row(row)
    }

    async fn find_by_id(&self, id: &UserId) -> DomainResult<User> {
        let row: UserRow = sqlx::query_as(
            "SELECT id, username, email, password_hash, role, disabled, created_at, updated_at, password_changed_at FROM users WHERE id = ?"
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::NotFound { resource: "user".to_string() },
            _ => DomainError::Internal { message: format!("Failed to find user: {}", e) },
        })?;

        user_from_row(row)
    }

    async fn update_email(&self, id: &UserId, email: &EmailAddress) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query("UPDATE users SET email = ?, updated_at = ? WHERE id = ?")
            .bind(email.as_str())
            .bind(now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to update email: {}", e),
            })?;
        Ok(())
    }

    async fn update_password(&self, id: &UserId, password_hash: &str) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query(
            "UPDATE users SET password_hash = ?, password_changed_at = ?, updated_at = ? WHERE id = ?"
        )
        .bind(password_hash)
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to update password: {}", e),
        })?;
        Ok(())
    }

    async fn disable_user(&self, id: &UserId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query("UPDATE users SET disabled = ?, updated_at = ? WHERE id = ?")
            .bind(true)
            .bind(now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to disable user: {}", e),
            })?;
        Ok(())
    }

    async fn list_users(&self, limit: i64, offset: i64) -> DomainResult<Vec<User>> {
        let rows: Vec<UserRow> = sqlx::query_as(
            "SELECT id, username, email, password_hash, role, disabled, created_at, updated_at, password_changed_at FROM users ORDER BY created_at DESC LIMIT ? OFFSET ?"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list users: {}", e),
        })?;

        let mut users = Vec::new();
        for row in rows {
            users.push(user_from_row(row)?);
        }

        Ok(users)
    }

    async fn list_transfer_targets(&self, exclude: &UserId) -> DomainResult<Vec<User>> {
        let rows: Vec<UserRow> = sqlx::query_as(
            "SELECT id, username, email, password_hash, role, disabled, created_at, updated_at, password_changed_at FROM users WHERE id != ? AND disabled = 0 ORDER BY username ASC LIMIT 500"
        )
        .bind(exclude.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list transfer targets: {}", e),
        })?;

        rows.into_iter().map(user_from_row).collect()
    }
}

#[derive(Debug)]
pub struct SqliteNamespaceRepo {
    pool: SqlitePool,
}

impl SqliteNamespaceRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl NamespaceRepo for SqliteNamespaceRepo {
    async fn create_default(&self, owner_id: &UserId, slug: &str) -> DomainResult<NamespaceId> {
        let id = NamespaceId::new();
        sqlx::query("INSERT INTO namespaces (id, slug, owner_user_id) VALUES (?, ?, ?)")
            .bind(id.to_string())
            .bind(slug)
            .bind(owner_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                    DomainError::Conflict {
                        message: format!("Namespace '{}' already exists for owner", slug),
                    }
                }
                _ => DomainError::Internal {
                    message: format!("Failed to create namespace: {}", e),
                },
            })?;
        Ok(id)
    }

    async fn find_default(&self) -> DomainResult<NamespaceId> {
        let row: (String,) = sqlx::query_as(
            "SELECT id FROM namespaces WHERE slug = ? ORDER BY created_at ASC, id ASC LIMIT 1",
        )
        .bind("default")
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::NotFound {
                resource: "namespace".to_string(),
            },
            _ => DomainError::Internal {
                message: format!("Failed to find default namespace: {}", e),
            },
        })?;

        let uuid = uuid::Uuid::parse_str(&row.0).map_err(|_| DomainError::Internal {
            message: "Invalid UUID".to_string(),
        })?;

        Ok(NamespaceId::from_uuid(uuid))
    }

    async fn find_default_for_owner(&self, owner_id: &UserId) -> DomainResult<NamespaceId> {
        let row: (String,) = sqlx::query_as(
            "SELECT id FROM namespaces WHERE owner_user_id = ? AND slug = ? LIMIT 1",
        )
        .bind(owner_id.to_string())
        .bind("default")
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::NotFound {
                resource: "namespace".to_string(),
            },
            _ => DomainError::Internal {
                message: format!("Failed to find default namespace for owner: {}", e),
            },
        })?;

        let uuid = uuid::Uuid::parse_str(&row.0).map_err(|_| DomainError::Internal {
            message: "Invalid UUID".to_string(),
        })?;

        Ok(NamespaceId::from_uuid(uuid))
    }
}

#[derive(Debug)]
pub struct SqliteSystemSettingsRepo {
    pool: SqlitePool,
}

impl SqliteSystemSettingsRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl SystemSettingsRepo for SqliteSystemSettingsRepo {
    async fn set_bootstrapped(&self) -> DomainResult<()> {
        sqlx::query("INSERT OR REPLACE INTO system_settings (key, value_json) VALUES (?, ?)")
            .bind("bootstrapped")
            .bind("true")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to set bootstrapped: {}", e),
            })?;
        Ok(())
    }

    async fn is_bootstrapped(&self) -> DomainResult<bool> {
        let result: Option<(String,)> =
            sqlx::query_as("SELECT value_json FROM system_settings WHERE key = ?")
                .bind("bootstrapped")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to check bootstrapped: {}", e),
                })?;
        Ok(result.map(|(v,)| v == "true").unwrap_or(false))
    }
}

#[derive(Debug)]
pub struct SqliteSessionRepo {
    pool: SqlitePool,
}

impl SqliteSessionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl Clone for SqliteSessionRepo {
    fn clone(&self) -> Self {
        // For HTTP layer, we need Clone. Use Arc<SqlitePool> in production
        Self {
            pool: self.pool.clone(),
        }
    }
}

#[async_trait::async_trait]
impl SessionRepo for SqliteSessionRepo {
    async fn create_session(
        &self,
        user_id: &UserId,
        session_token_hash: &str,
        expires_at: time::OffsetDateTime,
        user_agent: Option<&str>,
        ip_addr: Option<&str>,
    ) -> DomainResult<SessionId> {
        let id = SessionId::new();
        let now = time::OffsetDateTime::now_utc();
        sqlx::query(
            "INSERT INTO user_sessions (id, user_id, session_token_hash, issued_at, expires_at, user_agent, ip_addr, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(session_token_hash)
        .bind(now)
        .bind(expires_at)
        .bind(user_agent)
        .bind(ip_addr)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to create session: {}", e),
        })?;
        Ok(id)
    }

    async fn find_session_by_token_hash(&self, token_hash: &str) -> DomainResult<UserSession> {
        let row: (String, String, String, String, String, Option<String>, Option<String>, String, Option<String>) = sqlx::query_as(
            "SELECT id, user_id, session_token_hash, issued_at, expires_at, revoked_at, user_agent, ip_addr, last_seen_at FROM user_sessions WHERE session_token_hash = ?"
        )
        .bind(token_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::NotFound { resource: "session".to_string() },
            _ => DomainError::Internal { message: format!("Failed to find session: {}", e) },
        })?;

        Ok(UserSession {
            id: SessionId::from_uuid(uuid::Uuid::parse_str(&row.0).map_err(|_| {
                DomainError::Internal {
                    message: "Invalid UUID".to_string(),
                }
            })?),
            user_id: UserId::from_uuid(uuid::Uuid::parse_str(&row.1).map_err(|_| {
                DomainError::Internal {
                    message: "Invalid UUID".to_string(),
                }
            })?),
            session_token_hash: row.2,
            issued_at: time::OffsetDateTime::parse(
                &row.3,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| DomainError::Internal {
                message: "Invalid timestamp".to_string(),
            })?,
            expires_at: time::OffsetDateTime::parse(
                &row.4,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| DomainError::Internal {
                message: "Invalid timestamp".to_string(),
            })?,
            revoked_at: row
                .5
                .as_ref()
                .map(|s| {
                    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                })
                .transpose()
                .map_err(|_| DomainError::Internal {
                    message: "Invalid timestamp".to_string(),
                })?,
            user_agent: row.6,
            ip_addr: Some(row.7),
            last_seen_at: time::OffsetDateTime::parse(
                row.8.as_ref().unwrap_or(&"".to_string()),
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| DomainError::Internal {
                message: "Invalid timestamp".to_string(),
            })?,
        })
    }

    async fn update_last_seen(&self, id: &SessionId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query("UPDATE user_sessions SET last_seen_at = ? WHERE id = ?")
            .bind(now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to update last seen: {}", e),
            })?;
        Ok(())
    }

    async fn revoke_session(&self, id: &SessionId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query("UPDATE user_sessions SET revoked_at = ? WHERE id = ?")
            .bind(now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to revoke session: {}", e),
            })?;
        Ok(())
    }

    async fn revoke_user_sessions(&self, user_id: &UserId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query(
            "UPDATE user_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to revoke user sessions: {}", e),
        })?;
        Ok(())
    }

    async fn cleanup_expired_sessions(&self) -> DomainResult<i64> {
        let now = time::OffsetDateTime::now_utc();
        let result = sqlx::query(
            "DELETE FROM user_sessions WHERE expires_at < ? OR (revoked_at IS NOT NULL AND revoked_at < ?)"
        )
        .bind(now)
        .bind(now - time::Duration::days(30)) // Keep revoked sessions for 30 days
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to cleanup sessions: {}", e),
        })?;
        Ok(result.rows_affected() as i64)
    }
}

// File system repositories

#[derive(Debug, Clone)]
pub struct SqliteEntryRepo {
    pool: SqlitePool,
}

impl SqliteEntryRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

type EntryRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
);

fn parse_version_id_opt(value: Option<&str>) -> DomainResult<Option<VersionId>> {
    value
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            uuid::Uuid::parse_str(raw)
                .map(VersionId::from_uuid)
                .map_err(|_| DomainError::Internal {
                    message: "Invalid UUID".to_string(),
                })
        })
        .transpose()
}

fn parse_blob_id_opt(value: Option<&str>) -> DomainResult<Option<BlobId>> {
    value
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            uuid::Uuid::parse_str(raw)
                .map(BlobId::from_uuid)
                .map_err(|_| DomainError::Internal {
                    message: "Invalid UUID".to_string(),
                })
        })
        .transpose()
}

fn parse_user_id_opt(value: Option<&str>) -> DomainResult<Option<UserId>> {
    value
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            uuid::Uuid::parse_str(raw)
                .map(UserId::from_uuid)
                .map_err(|_| DomainError::Internal {
                    message: "Invalid UUID".to_string(),
                })
        })
        .transpose()
}

fn parse_timestamp(value: &str) -> DomainResult<time::OffsetDateTime> {
    if let Ok(parsed) =
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
    {
        return Ok(parsed);
    }

    // 兼容 SQLite `datetime('now')` 的 "YYYY-MM-DD HH:MM:SS"（按 UTC 解释）。
    // 例如运维手工补写的数据；解析失败不应让整张表都读不出来。
    let sqlite_format = time::format_description::parse_borrowed::<2>(
        "[year]-[month]-[day] [hour]:[minute]:[second]",
    );
    if let Ok(format) = sqlite_format
        && let Ok(primitive) = time::PrimitiveDateTime::parse(value, &format)
    {
        return Ok(primitive.assume_utc());
    }

    Err(DomainError::Internal {
        message: format!("Invalid timestamp: {value}"),
    })
}

fn parse_timestamp_opt(value: Option<&str>) -> DomainResult<Option<time::OffsetDateTime>> {
    value.map(parse_timestamp).transpose()
}

fn parse_entry_kind(value: &str) -> DomainResult<EntryKind> {
    match value {
        "file" => Ok(EntryKind::File),
        "directory" => Ok(EntryKind::Directory),
        _ => Err(DomainError::Internal {
            message: "Invalid entry kind".to_string(),
        }),
    }
}

fn entry_kind_as_str(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Directory => "directory",
    }
}

fn parse_change_type(value: &str) -> DomainResult<ChangeType> {
    match value {
        "added" => Ok(ChangeType::Added),
        "modified" => Ok(ChangeType::Modified),
        "deleted" => Ok(ChangeType::Deleted),
        "renamed" => Ok(ChangeType::Renamed),
        _ => Err(DomainError::Internal {
            message: "Invalid change type".to_string(),
        }),
    }
}

fn change_type_as_str(change_type: ChangeType) -> &'static str {
    match change_type {
        ChangeType::Added => "added",
        ChangeType::Modified => "modified",
        ChangeType::Deleted => "deleted",
        ChangeType::Renamed => "renamed",
    }
}

fn entry_name(path: &str) -> String {
    path.split('/').next_back().unwrap_or("").to_string()
}

fn parse_entry_row(row: EntryRow) -> DomainResult<Entry> {
    let current_version_id = parse_version_id_opt(row.6.as_deref())?;

    Ok(Entry {
        id: EntryId::from_uuid(uuid::Uuid::parse_str(&row.0).map_err(|_| {
            DomainError::Internal {
                message: "Invalid UUID".to_string(),
            }
        })?),
        namespace_id: NamespaceId::from_uuid(uuid::Uuid::parse_str(&row.1).map_err(|_| {
            DomainError::Internal {
                message: "Invalid UUID".to_string(),
            }
        })?),
        parent_entry_id: None,
        path_norm: NormalizedPath::new(&row.2).map_err(|_| DomainError::Internal {
            message: "Invalid path".to_string(),
        })?,
        name: entry_name(&row.2),
        entry_type: parse_entry_kind(&row.3)?,
        current_version_id,
        created_at: parse_timestamp(&row.4)?,
        deleted_at: None,
    })
}

fn default_content_hash() -> ContentHash {
    ContentHash::new(&"0".repeat(64)).expect("64 zeros should be a valid sha256 string")
}

fn blob_storage_key(blob_id: &BlobId) -> String {
    let hash = blob_id.to_string();
    let dir = &hash[0..2];
    let filename = &hash[2..];
    format!("blobs/{}/{}", dir, filename)
}

fn is_text_content_type(mime_type: Option<&str>) -> bool {
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

type EntryVersionRow = (
    String,
    String,
    i64,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
);

fn parse_entry_version_row(
    (
        id,
        entry_id,
        version,
        blob_id,
        size,
        content_type,
        content_hash,
        created_at,
        created_by,
        message,
    ): EntryVersionRow,
) -> DomainResult<EntryVersion> {
    Ok(EntryVersion {
        id: VersionId::from_uuid(uuid::Uuid::parse_str(&id).map_err(|_| {
            DomainError::Internal {
                message: "Invalid UUID".to_string(),
            }
        })?),
        entry_id: EntryId::from_uuid(uuid::Uuid::parse_str(&entry_id).map_err(|_| {
            DomainError::Internal {
                message: "Invalid UUID".to_string(),
            }
        })?),
        version_no: version as u32,
        blob_id: parse_blob_id_opt(blob_id.as_deref())?,
        size_bytes: ByteSize::new(size.unwrap_or_default() as u64),
        mime_type: content_type.clone(),
        is_text: is_text_content_type(content_type.as_deref()),
        content_hash: content_hash
            .as_deref()
            .map(ContentHash::new)
            .transpose()
            .map_err(|_| DomainError::Internal {
                message: "Invalid content hash".to_string(),
            })?
            .unwrap_or_else(default_content_hash),
        created_by: UserId::from_uuid(uuid::Uuid::parse_str(&created_by).map_err(|_| {
            DomainError::Internal {
                message: "Invalid UUID".to_string(),
            }
        })?),
        created_at: parse_timestamp(&created_at)?,
        change_type: if version <= 1 {
            ChangeType::Added
        } else {
            ChangeType::Modified
        },
        change_message: message
            .as_deref()
            .and_then(|value| NonEmptyMessage::new(value).ok()),
        source_upload_id: None,
    })
}

#[async_trait::async_trait]
impl EntryRepo for SqliteEntryRepo {
    async fn find_by_id(&self, entry_id: &EntryId) -> DomainResult<Entry> {
        let row: EntryRow = sqlx::query_as(
            r#"
            SELECT
                e.id,
                e.namespace_id,
                e.path,
                e.kind,
                e.created_at,
                e.updated_at,
                (
                    SELECT ev.id
                    FROM entry_versions ev
                    WHERE ev.entry_id = e.id
                    ORDER BY ev.version DESC
                    LIMIT 1
                ) AS current_version_id
            FROM entries e
            WHERE e.id = ?
            LIMIT 1
            "#,
        )
        .bind(entry_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::EntryNotFound,
            _ => DomainError::Internal {
                message: format!("Failed to find entry by id: {}", e),
            },
        })?;

        parse_entry_row(row)
    }

    async fn find_by_path(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
    ) -> DomainResult<Option<Entry>> {
        let row: Option<EntryRow> = sqlx::query_as(
            r#"
            SELECT
                e.id,
                e.namespace_id,
                e.path,
                e.kind,
                e.created_at,
                e.updated_at,
                (
                    SELECT ev.id
                    FROM entry_versions ev
                    WHERE ev.entry_id = e.id
                    ORDER BY ev.version DESC
                    LIMIT 1
                ) AS current_version_id
            FROM entries e
            WHERE e.namespace_id = ? AND e.path = ?
            LIMIT 1
            "#,
        )
        .bind(namespace_id.to_string())
        .bind(path.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to find entry: {}", e),
        })?;

        row.map(parse_entry_row).transpose()
    }

    async fn find_children(
        &self,
        namespace_id: &NamespaceId,
        parent_path: &NormalizedPath,
    ) -> DomainResult<Vec<Entry>> {
        let rows: Vec<EntryRow> = if parent_path.as_str().is_empty() {
            sqlx::query_as(
                r#"
                SELECT
                    e.id,
                    e.namespace_id,
                    e.path,
                    e.kind,
                    e.created_at,
                    e.updated_at,
                    (
                        SELECT ev.id
                        FROM entry_versions ev
                        WHERE ev.entry_id = e.id
                        ORDER BY ev.version DESC
                        LIMIT 1
                    ) AS current_version_id
                FROM entries e
                WHERE e.namespace_id = ?
                  AND instr(e.path, '/') = 0
                ORDER BY e.path
                "#,
            )
            .bind(namespace_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to find children: {}", e),
            })?
        } else {
            let direct_prefix = format!("{}/", parent_path.as_str().trim_end_matches('/'));
            sqlx::query_as(
                r#"
                SELECT
                    e.id,
                    e.namespace_id,
                    e.path,
                    e.kind,
                    e.created_at,
                    e.updated_at,
                    (
                        SELECT ev.id
                        FROM entry_versions ev
                        WHERE ev.entry_id = e.id
                        ORDER BY ev.version DESC
                        LIMIT 1
                    ) AS current_version_id
                FROM entries e
                WHERE e.namespace_id = ?
                  AND e.path LIKE ?
                                    AND instr(substr(e.path, length(?) + 1), '/') = 0
                ORDER BY e.path
                "#,
            )
            .bind(namespace_id.to_string())
            .bind(format!("{}%", direct_prefix))
            .bind(direct_prefix)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to find children: {}", e),
            })?
        };

        rows.into_iter().map(parse_entry_row).collect()
    }

    async fn find_all(&self, namespace_id: &NamespaceId) -> DomainResult<Vec<Entry>> {
        let rows: Vec<EntryRow> = sqlx::query_as(
            r#"
            SELECT
                e.id,
                e.namespace_id,
                e.path,
                e.kind,
                e.created_at,
                e.updated_at,
                (
                    SELECT ev.id
                    FROM entry_versions ev
                    WHERE ev.entry_id = e.id
                    ORDER BY ev.version DESC
                    LIMIT 1
                ) AS current_version_id
            FROM entries e
            WHERE e.namespace_id = ?
            ORDER BY e.path
            "#,
        )
        .bind(namespace_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to find all entries: {}", e),
        })?;

        rows.into_iter().map(parse_entry_row).collect()
    }

    async fn stats(&self, namespace_id: &NamespaceId) -> DomainResult<NamespaceStats> {
        #[derive(sqlx::FromRow)]
        struct StatsRow {
            file_count: i64,
            directory_count: i64,
            total_bytes: i64,
        }

        // 只统计当前版本（version 最大的一条）的大小；目录没有版本，按 0 计。
        // entries 表没有 current_version_id 列，这里用子查询取最新版本。
        let row: StatsRow = sqlx::query_as(
            r#"
            SELECT
                COUNT(CASE WHEN e.kind = 'file' THEN 1 END) as file_count,
                COUNT(CASE WHEN e.kind = 'directory' THEN 1 END) as directory_count,
                COALESCE(
                    SUM(
                        CASE WHEN e.kind = 'file' THEN (
                            SELECT ev.size FROM entry_versions ev
                            WHERE ev.entry_id = e.id
                            ORDER BY ev.version DESC
                            LIMIT 1
                        ) ELSE 0 END
                    ),
                    0
                ) as total_bytes
            FROM entries e
            WHERE e.namespace_id = ?
            "#,
        )
        .bind(namespace_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to load namespace stats: {e}"),
        })?;

        Ok(NamespaceStats {
            file_count: row.file_count.max(0) as u64,
            directory_count: row.directory_count.max(0) as u64,
            total_bytes: row.total_bytes.max(0) as u64,
        })
    }

    async fn stats_by_category(
        &self,
        namespace_id: &NamespaceId,
    ) -> DomainResult<Vec<CategoryUsage>> {
        #[derive(sqlx::FromRow)]
        struct CategoryRow {
            category: String,
            bytes: i64,
            file_count: i64,
        }

        // 分类规则与主流网盘一致：按当前版本的 MIME 前缀归类，
        // 文档包含文本、PDF 与 Office 系列；缺失类型归入 other。
        let rows: Vec<CategoryRow> = sqlx::query_as(
            r#"
            SELECT
                CASE
                    WHEN ev.content_type LIKE 'image/%' THEN 'image'
                    WHEN ev.content_type LIKE 'video/%' THEN 'video'
                    WHEN ev.content_type LIKE 'audio/%' THEN 'audio'
                    WHEN ev.content_type LIKE 'text/%'
                      OR ev.content_type IN (
                            'application/pdf',
                            'application/json',
                            'application/xml',
                            'application/rtf',
                            'application/msword',
                            'application/vnd.ms-excel',
                            'application/vnd.ms-powerpoint'
                      )
                      OR ev.content_type LIKE 'application/vnd.openxmlformats%'
                      OR ev.content_type LIKE 'application/vnd.oasis%'
                      THEN 'document'
                    ELSE 'other'
                END AS category,
                COALESCE(SUM(ev.size), 0) AS bytes,
                COUNT(*) AS file_count
            FROM entries e
            JOIN entry_versions ev ON ev.entry_id = e.id
            WHERE e.namespace_id = ?
              AND e.kind = 'file'
              AND ev.version = (
                    SELECT MAX(version) FROM entry_versions WHERE entry_id = e.id
              )
            GROUP BY category
            ORDER BY bytes DESC
            "#,
        )
        .bind(namespace_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to load category stats: {e}"),
        })?;

        Ok(rows
            .into_iter()
            .map(|row| CategoryUsage {
                category: FileCategory::from_sql(&row.category),
                bytes: row.bytes.max(0) as u64,
                file_count: row.file_count.max(0) as u64,
            })
            .collect())
    }

    async fn recent_files(
        &self,
        namespace_id: &NamespaceId,
        limit: u32,
    ) -> DomainResult<Vec<Entry>> {
        let rows: Vec<EntryRow> = sqlx::query_as(
            r#"
            SELECT
                e.id,
                e.namespace_id,
                e.path,
                e.kind,
                e.created_at,
                e.updated_at,
                (
                    SELECT ev.id FROM entry_versions ev
                    WHERE ev.entry_id = e.id
                    ORDER BY ev.version DESC
                    LIMIT 1
                ) AS current_version_id
            FROM entries e
            WHERE e.namespace_id = ? AND e.kind = 'file'
            ORDER BY (
                SELECT ev.created_at FROM entry_versions ev
                WHERE ev.entry_id = e.id
                ORDER BY ev.version DESC
                LIMIT 1
            ) DESC, e.path ASC
            LIMIT ?
            "#,
        )
        .bind(namespace_id.to_string())
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list recent files: {e}"),
        })?;

        rows.into_iter().map(parse_entry_row).collect()
    }

    async fn find_children_page(
        &self,
        namespace_id: &NamespaceId,
        parent_path: &NormalizedPath,
        limit: u32,
        offset: u32,
    ) -> DomainResult<(Vec<Entry>, u64)> {
        // sqlx 要求静态 SQL，因此根目录与子目录各写一份完整语句（均命中
        // (namespace_id, path) 索引）；排序与 live tree 的「目录优先 + 名称升序」一致。
        const TOTAL_ROOT: &str =
            "SELECT COUNT(*) FROM entries e WHERE e.namespace_id = ? AND instr(e.path, '/') = 0";
        // 只统计**直接子条目**：用 LIKE 前缀 + 「前缀之后不再含 /」过滤，
        // 否则会把整棵子树都算进来（`a/` 的范围匹配会命中 `a/docs/x`）。
        const TOTAL_PREFIX: &str = "SELECT COUNT(*) FROM entries e WHERE e.namespace_id = ? AND e.path LIKE ? AND instr(substr(e.path, length(?) + 1), '/') = 0";
        const PAGE_ROOT: &str = r#"
            SELECT
                e.id, e.namespace_id, e.path, e.kind, e.created_at, e.updated_at,
                (
                    SELECT ev.id FROM entry_versions ev
                    WHERE ev.entry_id = e.id
                    ORDER BY ev.version DESC
                    LIMIT 1
                ) AS current_version_id
            FROM entries e
            WHERE e.namespace_id = ? AND instr(e.path, '/') = 0
            ORDER BY (e.kind = 'directory') DESC, e.path ASC
            LIMIT ? OFFSET ?
        "#;
        const PAGE_PREFIX: &str = r#"
            SELECT
                e.id, e.namespace_id, e.path, e.kind, e.created_at, e.updated_at,
                (
                    SELECT ev.id FROM entry_versions ev
                    WHERE ev.entry_id = e.id
                    ORDER BY ev.version DESC
                    LIMIT 1
                ) AS current_version_id
            FROM entries e
            WHERE e.namespace_id = ? AND e.path LIKE ?
              AND instr(substr(e.path, length(?) + 1), '/') = 0
            ORDER BY (e.kind = 'directory') DESC, e.path ASC
            LIMIT ? OFFSET ?
        "#;
        let is_root = parent_path.as_str().is_empty();
        let root = parent_path.as_str().trim_end_matches('/');
        // LIKE 前缀与长度参数：`instr(substr(path, length(prefix)+1), '/') = 0`
        // 保证只取直接子条目
        let direct_prefix = format!("{root}/");
        let like_pattern = format!("{direct_prefix}%");

        let total: i64 = if is_root {
            sqlx::query_scalar::<_, i64>(TOTAL_ROOT)
                .bind(namespace_id.to_string())
                .fetch_one(&self.pool)
                .await
        } else {
            sqlx::query_scalar::<_, i64>(TOTAL_PREFIX)
                .bind(namespace_id.to_string())
                .bind(&like_pattern)
                .bind(&direct_prefix)
                .fetch_one(&self.pool)
                .await
        }
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to count children: {e}"),
        })?;

        let rows: Vec<EntryRow> = if is_root {
            sqlx::query_as::<_, EntryRow>(PAGE_ROOT)
                .bind(namespace_id.to_string())
                .bind(i64::from(limit))
                .bind(i64::from(offset))
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query_as::<_, EntryRow>(PAGE_PREFIX)
                .bind(namespace_id.to_string())
                .bind(&like_pattern)
                .bind(&direct_prefix)
                .bind(i64::from(limit))
                .bind(i64::from(offset))
                .fetch_all(&self.pool)
                .await
        }
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list children page: {e}"),
        })?;

        let entries = rows
            .into_iter()
            .map(parse_entry_row)
            .collect::<DomainResult<Vec<Entry>>>()?;

        Ok((entries, total.max(0) as u64))
    }

    async fn find_subtree(
        &self,
        namespace_id: &NamespaceId,
        root_path: &NormalizedPath,
    ) -> DomainResult<Vec<Entry>> {
        let root = root_path.as_str().trim_end_matches('/');
        // 范围比较（而非 LIKE）以便命中 (namespace_id, path) 索引：
        // root 自身取相等，后代形如 "root/..."，因此 "root/" <= path < "root0"。
        let lower = format!("{}/", root);
        let upper = format!("{}0", root);

        let rows: Vec<EntryRow> = sqlx::query_as(
            r#"
            SELECT
                e.id,
                e.namespace_id,
                e.path,
                e.kind,
                e.created_at,
                e.updated_at,
                (
                    SELECT ev.id
                    FROM entry_versions ev
                    WHERE ev.entry_id = e.id
                    ORDER BY ev.version DESC
                    LIMIT 1
                ) AS current_version_id
            FROM entries e
            WHERE e.namespace_id = ?
              AND (e.path = ? OR (e.path >= ? AND e.path < ?))
            ORDER BY e.path
            "#,
        )
        .bind(namespace_id.to_string())
        .bind(root)
        .bind(&lower)
        .bind(&upper)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to find subtree: {}", e),
        })?;

        rows.into_iter().map(parse_entry_row).collect()
    }

    async fn find_paths(
        &self,
        namespace_id: &NamespaceId,
        paths: &[NormalizedPath],
    ) -> DomainResult<Vec<Entry>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }

        const CHUNK: usize = 500;
        let mut entries = Vec::new();

        for chunk in paths.chunks(CHUNK) {
            let mut builder = sqlx::QueryBuilder::new(
                "SELECT \
                    e.id, e.namespace_id, e.path, e.kind, e.created_at, e.updated_at, \
                    ( \
                        SELECT ev.id \
                        FROM entry_versions ev \
                        WHERE ev.entry_id = e.id \
                        ORDER BY ev.version DESC \
                        LIMIT 1 \
                    ) AS current_version_id \
                 FROM entries e \
                 WHERE e.namespace_id = ",
            );
            builder.push_bind(namespace_id.to_string());
            builder.push(" AND e.path IN (");
            let mut separated = builder.separated(", ");
            for path in chunk {
                separated.push_bind(path.as_str().to_string());
            }
            separated.push_unseparated(")");

            let rows: Vec<EntryRow> = builder
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to find paths: {}", e),
                })?;

            for row in rows {
                entries.push(parse_entry_row(row)?);
            }
        }

        Ok(entries)
    }

    async fn referenced_blob_ids(&self) -> DomainResult<Vec<BlobId>> {
        let rows: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT blob_id FROM entry_versions WHERE blob_id IS NOT NULL
            UNION
            SELECT blob_id FROM snapshot_entries WHERE blob_id IS NOT NULL
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list referenced blobs: {}", e),
        })?;

        rows.iter()
            .map(|id| {
                BlobId::from_string(id).map_err(|_| DomainError::Internal {
                    message: "Invalid blob id".to_string(),
                })
            })
            .collect()
    }

    async fn create_entry(
        &self,
        namespace_id: &NamespaceId,
        path: &NormalizedPath,
        kind: EntryKind,
        _user_id: &UserId,
    ) -> DomainResult<EntryId> {
        let id = EntryId::new();
        let now = time::OffsetDateTime::now_utc();
        let kind_str = match kind {
            EntryKind::File => "file",
            EntryKind::Directory => "directory",
        };

        sqlx::query(
            "INSERT INTO entries (id, namespace_id, path, kind, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(namespace_id.to_string())
        .bind(path.as_str())
        .bind(kind_str)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                DomainError::PathConflict {
                    message: format!("Path already exists: {}", path.as_str()),
                }
            }
            _ => DomainError::Internal {
                message: format!("Failed to create entry: {}", e),
            },
        })?;
        Ok(id)
    }

    async fn update_current_version(
        &self,
        entry_id: &EntryId,
        version_id: &VersionId,
    ) -> DomainResult<()> {
        let version_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entry_versions WHERE id = ? AND entry_id = ?")
                .bind(version_id.to_string())
                .bind(entry_id.to_string())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to validate version ownership: {}", e),
                })?;

        if version_exists == 0 {
            return Err(DomainError::VersionNotFound);
        }

        let now = time::OffsetDateTime::now_utc();
        let result = sqlx::query("UPDATE entries SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(entry_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to update current version: {}", e),
            })?;

        if result.rows_affected() == 0 {
            return Err(DomainError::EntryNotFound);
        }

        Ok(())
    }

    async fn delete_entry(&self, entry_id: &EntryId) -> DomainResult<()> {
        sqlx::query("DELETE FROM entries WHERE id = ?")
            .bind(entry_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to delete entry: {}", e),
            })?;
        Ok(())
    }

    async fn delete_entries(&self, entry_ids: &[EntryId]) -> DomainResult<()> {
        if entry_ids.is_empty() {
            return Ok(());
        }

        const CHUNK: usize = 500;
        for chunk in entry_ids.chunks(CHUNK) {
            let mut builder = sqlx::QueryBuilder::new("DELETE FROM entries WHERE id IN (");
            let mut separated = builder.separated(", ");
            for entry_id in chunk {
                separated.push_bind(entry_id.to_string());
            }
            separated.push_unseparated(")");

            builder
                .build()
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to delete entries: {}", e),
                })?;
        }

        Ok(())
    }

    async fn release_blob_references(
        &self,
        references: &[(BlobId, u32)],
    ) -> DomainResult<Vec<BlobId>> {
        if references.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = self.pool.begin().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to begin blob release transaction: {}", e),
        })?;
        let mut removable = Vec::new();

        for (blob_id, ref_count) in references {
            if *ref_count == 0 {
                continue;
            }

            sqlx::query("UPDATE blobs SET ref_count = MAX(ref_count - ?, 0) WHERE id = ?")
                .bind(i64::from(*ref_count))
                .bind(blob_id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to decrement blob reference count: {}", e),
                })?;

            let remaining: Option<i64> =
                sqlx::query_scalar("SELECT ref_count FROM blobs WHERE id = ? LIMIT 1")
                    .bind(blob_id.to_string())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to fetch blob reference count: {}", e),
                    })?;

            if remaining == Some(0) {
                sqlx::query("DELETE FROM blobs WHERE id = ?")
                    .bind(blob_id.to_string())
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to delete blob metadata: {}", e),
                    })?;
                removable.push(*blob_id);
            }
        }

        tx.commit().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to commit blob release transaction: {}", e),
        })?;

        Ok(removable)
    }

    async fn move_entry(&self, entry_id: &EntryId, new_path: &NormalizedPath) -> DomainResult<()> {
        sqlx::query("UPDATE entries SET path = ? WHERE id = ?")
            .bind(new_path.as_str())
            .bind(entry_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                    DomainError::PathConflict {
                        message: format!("Path already exists: {}", new_path.as_str()),
                    }
                }
                _ => DomainError::Internal {
                    message: format!("Failed to move entry: {}", e),
                },
            })?;
        Ok(())
    }

    async fn transfer_entries(&self, moves: &[(EntryId, NamespaceId)]) -> DomainResult<()> {
        if moves.is_empty() {
            return Ok(());
        }

        let now = time::OffsetDateTime::now_utc();
        let mut tx = self.pool.begin().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to begin transfer transaction: {}", e),
        })?;

        for (entry_id, namespace_id) in moves {
            let result =
                sqlx::query("UPDATE entries SET namespace_id = ?, updated_at = ? WHERE id = ?")
                    .bind(namespace_id.to_string())
                    .bind(now)
                    .bind(entry_id.to_string())
                    .execute(&mut *tx)
                    .await;

            match result {
                Ok(_) => {}
                // (namespace_id, path) 唯一约束：目标命名空间下已有同名路径
                Err(sqlx::Error::Database(ref db_err)) if db_err.is_unique_violation() => {
                    return Err(DomainError::PathConflict {
                        message: "target namespace already has an entry at this path".to_string(),
                    });
                }
                Err(e) => {
                    return Err(DomainError::Internal {
                        message: format!("Failed to transfer entry: {}", e),
                    });
                }
            }
        }

        tx.commit().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to commit transfer transaction: {}", e),
        })?;

        Ok(())
    }

    async fn move_entries(&self, moves: &[(EntryId, NormalizedPath)]) -> DomainResult<()> {
        if moves.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to begin move transaction: {}", e),
        })?;

        for (entry_id, new_path) in moves {
            sqlx::query("UPDATE entries SET path = ? WHERE id = ?")
                .bind(new_path.as_str())
                .bind(entry_id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|e| match e {
                    sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                        DomainError::PathConflict {
                            message: format!("Path already exists: {}", new_path.as_str()),
                        }
                    }
                    _ => DomainError::Internal {
                        message: format!("Failed to move entry: {}", e),
                    },
                })?;
        }

        tx.commit().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to commit move transaction: {}", e),
        })?;
        Ok(())
    }

    async fn get_entry_history(
        &self,
        entry_id: &EntryId,
        limit: u32,
        _cursor: Option<&str>,
    ) -> DomainResult<Vec<EntryVersion>> {
        let rows: Vec<EntryVersionRow> = sqlx::query_as(
            r#"
            SELECT
                ev.id,
                ev.entry_id,
                ev.version,
                ev.blob_id,
                ev.size,
                ev.content_type,
                b.content_hash,
                ev.created_at,
                ev.created_by,
                ev.message
            FROM entry_versions ev
            LEFT JOIN blobs b ON b.id = ev.blob_id
            WHERE ev.entry_id = ?
            ORDER BY version DESC
            LIMIT ?
            "#,
        )
        .bind(entry_id.to_string())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to get entry history: {}", e),
        })?;

        rows.into_iter().map(parse_entry_version_row).collect()
    }

    async fn find_version(&self, version_id: &VersionId) -> DomainResult<EntryVersion> {
        let row: EntryVersionRow = sqlx::query_as(
            r#"
            SELECT
                ev.id,
                ev.entry_id,
                ev.version,
                ev.blob_id,
                ev.size,
                ev.content_type,
                b.content_hash,
                ev.created_at,
                ev.created_by,
                ev.message
            FROM entry_versions ev
            LEFT JOIN blobs b ON b.id = ev.blob_id
            WHERE ev.id = ?
            LIMIT 1
            "#,
        )
        .bind(version_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::VersionNotFound,
            _ => DomainError::Internal {
                message: format!("Failed to find entry version: {}", e),
            },
        })?;

        parse_entry_version_row(row)
    }

    async fn find_versions(&self, version_ids: &[VersionId]) -> DomainResult<Vec<EntryVersion>> {
        if version_ids.is_empty() {
            return Ok(Vec::new());
        }

        // SQLite 对绑定参数数量有限制，分批查询。
        const CHUNK: usize = 500;
        let mut versions = Vec::with_capacity(version_ids.len());

        for chunk in version_ids.chunks(CHUNK) {
            let mut builder = sqlx::QueryBuilder::new(
                "SELECT \
                    ev.id, ev.entry_id, ev.version, ev.blob_id, ev.size, ev.content_type, \
                    b.content_hash, ev.created_at, ev.created_by, ev.message \
                 FROM entry_versions ev \
                 LEFT JOIN blobs b ON b.id = ev.blob_id \
                 WHERE ev.id IN (",
            );
            let mut separated = builder.separated(", ");
            for version_id in chunk {
                separated.push_bind(version_id.to_string());
            }
            separated.push_unseparated(")");

            let rows: Vec<EntryVersionRow> = builder
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::Internal {
                message: format!("Failed to find entry versions: {}", e),
            })?;

            for row in rows {
                versions.push(parse_entry_version_row(row)?);
            }
        }

        Ok(versions)
    }

    async fn find_versions_for_entries(
        &self,
        entry_ids: &[EntryId],
    ) -> DomainResult<Vec<EntryVersion>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }

        const CHUNK: usize = 500;
        let mut versions = Vec::new();

        for chunk in entry_ids.chunks(CHUNK) {
            let mut builder = sqlx::QueryBuilder::new(
                "SELECT \
                    ev.id, ev.entry_id, ev.version, ev.blob_id, ev.size, ev.content_type, \
                    b.content_hash, ev.created_at, ev.created_by, ev.message \
                 FROM entry_versions ev \
                 LEFT JOIN blobs b ON b.id = ev.blob_id \
                 WHERE ev.entry_id IN (",
            );
            let mut separated = builder.separated(", ");
            for entry_id in chunk {
                separated.push_bind(entry_id.to_string());
            }
            separated.push_unseparated(")");

            let rows: Vec<EntryVersionRow> = builder
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::Internal {
                message: format!("Failed to find versions for entries: {}", e),
            })?;

            for row in rows {
                versions.push(parse_entry_version_row(row)?);
            }
        }

        Ok(versions)
    }

    async fn create_version(
        &self,
        entry_id: &EntryId,
        blob_id: Option<&BlobId>,
        content_hash: Option<&ContentHash>,
        size_bytes: u64,
        mime_type: Option<&str>,
        created_by: &UserId,
        message: Option<&str>,
    ) -> DomainResult<EntryVersion> {
        let version_id = VersionId::new();
        let now = time::OffsetDateTime::now_utc();
        let next_version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM entry_versions WHERE entry_id = ?",
        )
        .bind(entry_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to calculate next entry version: {}", e),
        })?;

        let mut tx = self.pool.begin().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to begin entry version transaction: {}", e),
        })?;

        if let Some(blob_id) = blob_id {
            let content_hash = content_hash.ok_or_else(|| DomainError::Internal {
                message: "Blob content hash is required".to_string(),
            })?;
            sqlx::query(
                r#"
                INSERT INTO blobs (
                    id,
                    content_hash,
                    storage_key,
                    size,
                    content_type,
                    ref_count,
                    created_at,
                    uploaded_by,
                    verified_at
                ) VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    ref_count = blobs.ref_count + 1,
                    verified_at = excluded.verified_at,
                    content_type = COALESCE(blobs.content_type, excluded.content_type)
                "#,
            )
            .bind(blob_id.to_string())
            .bind(content_hash.as_str())
            .bind(blob_storage_key(blob_id))
            .bind(size_bytes as i64)
            .bind(mime_type)
            .bind(now)
            .bind(created_by.to_string())
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to register blob metadata: {}", e),
            })?;
        }

        sqlx::query(
            "INSERT INTO entry_versions (id, entry_id, version, blob_id, size, content_type, created_at, created_by, message) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(version_id.to_string())
        .bind(entry_id.to_string())
        .bind(next_version)
        .bind(blob_id.map(ToString::to_string))
        .bind(size_bytes as i64)
        .bind(mime_type)
        .bind(now)
        .bind(created_by.to_string())
        .bind(message)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to create entry version: {}", e),
        })?;

        sqlx::query("UPDATE entries SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(entry_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to update entry timestamp: {}", e),
            })?;

        tx.commit().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to commit entry version transaction: {}", e),
        })?;

        self.find_version(&version_id).await
    }
}

/// 收藏夹仓储：按条目 ID 记录，重命名/移动后依然有效。
#[derive(Debug, Clone)]
pub struct SqliteFavoriteRepo {
    pool: SqlitePool,
}

impl SqliteFavoriteRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl FavoriteRepo for SqliteFavoriteRepo {
    async fn list(&self, namespace_id: &NamespaceId) -> DomainResult<Vec<Entry>> {
        let rows: Vec<EntryRow> = sqlx::query_as(
            r#"
            SELECT
                e.id,
                e.namespace_id,
                e.path,
                e.kind,
                e.created_at,
                e.updated_at,
                (
                    SELECT ev.id FROM entry_versions ev
                    WHERE ev.entry_id = e.id
                    ORDER BY ev.version DESC
                    LIMIT 1
                ) AS current_version_id
            FROM favorites f
            JOIN entries e ON e.id = f.entry_id
            WHERE f.namespace_id = ?
            ORDER BY f.created_at DESC, e.path ASC
            "#,
        )
        .bind(namespace_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list favorites: {e}"),
        })?;

        rows.into_iter().map(parse_entry_row).collect()
    }

    async fn add(&self, namespace_id: &NamespaceId, entry_id: &EntryId) -> DomainResult<bool> {
        let result =
            sqlx::query("INSERT OR IGNORE INTO favorites (namespace_id, entry_id) VALUES (?, ?)")
                .bind(namespace_id.to_string())
                .bind(entry_id.to_string())
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to add favorite: {e}"),
                })?;

        Ok(result.rows_affected() > 0)
    }

    async fn remove(&self, namespace_id: &NamespaceId, entry_id: &EntryId) -> DomainResult<bool> {
        let result = sqlx::query("DELETE FROM favorites WHERE namespace_id = ? AND entry_id = ?")
            .bind(namespace_id.to_string())
            .bind(entry_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to remove favorite: {e}"),
            })?;

        Ok(result.rows_affected() > 0)
    }

    async fn contains(&self, namespace_id: &NamespaceId, entry_id: &EntryId) -> DomainResult<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM favorites WHERE namespace_id = ? AND entry_id = ?",
        )
        .bind(namespace_id.to_string())
        .bind(entry_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to check favorite: {e}"),
        })?;

        Ok(count > 0)
    }
}

#[derive(Debug)]
/// 审计日志仓储：只追加、只查询。表的触发器会拒绝任何 UPDATE/DELETE。
#[derive(Clone)]
pub struct SqliteAuditLogRepo {
    pool: SqlitePool,
}

impl SqliteAuditLogRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct AuditLogRow {
    id: String,
    created_at: String,
    user_id: Option<String>,
    username: String,
    action: String,
    result: String,
    target: Option<String>,
    ip: Option<String>,
    user_agent: Option<String>,
    device: Option<String>,
    detail: Option<String>,
}

fn parse_audit_row(row: AuditLogRow) -> DomainResult<AuditLog> {
    let created_at = parse_timestamp(&row.created_at)?;
    let user_id = match row.user_id {
        Some(raw) => Some(UserId::from_uuid(uuid::Uuid::parse_str(&raw).map_err(
            |e| DomainError::Internal {
                message: format!("Invalid audit user id: {e}"),
            },
        )?)),
        None => None,
    };

    Ok(AuditLog {
        id: row.id,
        created_at,
        user_id,
        username: row.username,
        action: row.action,
        result: AuditResult::from_sql(&row.result),
        target: row.target,
        ip: row.ip,
        user_agent: row.user_agent,
        device: row.device,
        detail: row.detail,
    })
}

#[async_trait::async_trait]
impl AuditLogRepo for SqliteAuditLogRepo {
    async fn append(&self, entry: &NewAuditLog) -> DomainResult<()> {
        let created_at = time::OffsetDateTime::now_utc();

        sqlx::query(
            r#"
            INSERT INTO audit_logs (
                id, created_at, user_id, username, action, result,
                target, ip, user_agent, device, detail
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(created_at)
        .bind(entry.user_id.as_ref().map(|id| id.to_string()))
        .bind(&entry.username)
        .bind(&entry.action)
        .bind(entry.result.as_str())
        .bind(&entry.target)
        .bind(&entry.ip)
        .bind(&entry.user_agent)
        .bind(&entry.device)
        .bind(&entry.detail)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to append audit log: {e}"),
        })?;

        Ok(())
    }

    async fn list(&self, query: &AuditLogQuery) -> DomainResult<AuditLogPage> {
        // 条件与绑定值保持一致顺序，供 count 与分页查询复用
        let keyword = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{value}%"));
        let action = query
            .action
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let result = query.result.map(|value| value.as_str().to_string());
        let since = query.since;
        let until = query.until;

        // sqlx 要求 SQL 为静态字符串（避免拼接注入），因此两处各写一遍完整语句
        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM audit_logs
            WHERE (?1 IS NULL OR username LIKE ?1 OR ip LIKE ?1)
              AND (?2 IS NULL OR action = ?2)
              AND (?3 IS NULL OR result = ?3)
              AND (?4 IS NULL OR created_at >= ?4)
              AND (?5 IS NULL OR created_at < ?5)
            "#,
        )
        .bind(keyword.as_deref())
        .bind(action.as_deref())
        .bind(result.as_deref())
        .bind(since)
        .bind(until)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to count audit logs: {e}"),
        })?;

        let limit = query.limit.clamp(1, 500) as i64;
        let offset = query.offset as i64;

        let rows: Vec<AuditLogRow> = sqlx::query_as(
            r#"
            SELECT id, created_at, user_id, username, action, result,
                   target, ip, user_agent, device, detail
            FROM audit_logs
            WHERE (?1 IS NULL OR username LIKE ?1 OR ip LIKE ?1)
              AND (?2 IS NULL OR action = ?2)
              AND (?3 IS NULL OR result = ?3)
              AND (?4 IS NULL OR created_at >= ?4)
              AND (?5 IS NULL OR created_at < ?5)
            ORDER BY created_at DESC, id DESC
            LIMIT ?6 OFFSET ?7
            "#,
        )
        .bind(keyword.as_deref())
        .bind(action.as_deref())
        .bind(result.as_deref())
        .bind(since)
        .bind(until)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list audit logs: {e}"),
        })?;

        let items = rows
            .into_iter()
            .map(parse_audit_row)
            .collect::<DomainResult<Vec<_>>>()?;

        Ok(AuditLogPage {
            items,
            total: total.max(0) as u64,
        })
    }

    async fn summarize(&self, query: &AuditLogQuery, top: u32) -> DomainResult<AuditLogSummary> {
        // 与 list 相同的过滤条件（sqlx 要求静态 SQL，因此这里各写一遍）
        let keyword = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{value}%"));
        let action = query
            .action
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let result = query.result.map(|value| value.as_str().to_string());
        let since = query.since;
        let until = query.until;
        let top = top.clamp(1, 50) as i64;

        let (total, failures): (i64, i64) = sqlx::query_as(
            r#"
            SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN result = 'failure' THEN 1 ELSE 0 END), 0)
            FROM audit_logs
            WHERE (?1 IS NULL OR username LIKE ?1 OR ip LIKE ?1)
              AND (?2 IS NULL OR action = ?2)
              AND (?3 IS NULL OR result = ?3)
              AND (?4 IS NULL OR created_at >= ?4)
              AND (?5 IS NULL OR created_at < ?5)
            "#,
        )
        .bind(keyword.as_deref())
        .bind(action.as_deref())
        .bind(result.as_deref())
        .bind(since)
        .bind(until)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to summarize audit logs: {e}"),
        })?;

        let users: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT
                CASE WHEN username = '' THEN '(匿名)' ELSE username END AS key,
                COUNT(*) AS count
            FROM audit_logs
            WHERE (?1 IS NULL OR username LIKE ?1 OR ip LIKE ?1)
              AND (?2 IS NULL OR action = ?2)
              AND (?3 IS NULL OR result = ?3)
              AND (?4 IS NULL OR created_at >= ?4)
              AND (?5 IS NULL OR created_at < ?5)
            GROUP BY key
            ORDER BY count DESC, key ASC
            LIMIT ?6
            "#,
        )
        .bind(keyword.as_deref())
        .bind(action.as_deref())
        .bind(result.as_deref())
        .bind(since)
        .bind(until)
        .bind(top)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to summarize audit users: {e}"),
        })?;

        let actions: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT action AS key, COUNT(*) AS count
            FROM audit_logs
            WHERE (?1 IS NULL OR username LIKE ?1 OR ip LIKE ?1)
              AND (?2 IS NULL OR action = ?2)
              AND (?3 IS NULL OR result = ?3)
              AND (?4 IS NULL OR created_at >= ?4)
              AND (?5 IS NULL OR created_at < ?5)
            GROUP BY action
            ORDER BY count DESC, key ASC
            LIMIT ?6
            "#,
        )
        .bind(keyword.as_deref())
        .bind(action.as_deref())
        .bind(result.as_deref())
        .bind(since)
        .bind(until)
        .bind(top)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to summarize audit actions: {e}"),
        })?;

        let to_counts = |rows: Vec<(String, i64)>| {
            rows.into_iter()
                .map(|(key, count)| AuditCount {
                    key,
                    count: count.max(0) as u64,
                })
                .collect()
        };

        Ok(AuditLogSummary {
            total: total.max(0) as u64,
            failures: failures.max(0) as u64,
            users: to_counts(users),
            actions: to_counts(actions),
        })
    }

    async fn distinct_actions(&self) -> DomainResult<Vec<String>> {
        let actions: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT action FROM audit_logs ORDER BY action ASC")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to list audit actions: {e}"),
                })?;

        Ok(actions)
    }
}

pub struct SqliteSnapshotRepo {
    pool: SqlitePool,
}

impl SqliteSnapshotRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn next_snapshot_no(&self, namespace_id: &NamespaceId) -> DomainResult<u32> {
        let snapshot_no: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                MAX(
                    CASE
                        WHEN instr(name, '-') > 0 THEN CAST(substr(name, instr(name, '-') + 1) AS INTEGER)
                        ELSE 0
                    END
                ),
                0
            ) + 1
            FROM snapshots
            WHERE namespace_id = ?
            "#,
        )
        .bind(namespace_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to calculate next snapshot number: {}", e),
        })?;

        u32::try_from(snapshot_no).map_err(|_| DomainError::Internal {
            message: format!("Invalid snapshot number: {}", snapshot_no),
        })
    }
}

impl Clone for SqliteSnapshotRepo {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

fn snapshot_name(kind: SnapshotKind, snapshot_no: u32) -> String {
    match kind {
        SnapshotKind::UserCreated => format!("snapshot-{:06}", snapshot_no),
        SnapshotKind::AutoCommit => format!("auto-{:06}", snapshot_no),
    }
}

const SNAPSHOT_NAME_RETRY_LIMIT: usize = 8;

fn parse_snapshot_name(name: &str) -> (SnapshotKind, u32) {
    if let Some(number) = name.strip_prefix("auto-") {
        let snapshot_no = number.parse::<u32>().unwrap_or(1);
        return (SnapshotKind::AutoCommit, snapshot_no);
    }

    if let Some(number) = name.strip_prefix("snapshot-") {
        let snapshot_no = number.parse::<u32>().unwrap_or(1);
        return (SnapshotKind::UserCreated, snapshot_no);
    }

    (SnapshotKind::UserCreated, 1)
}

#[cfg(test)]
mod snapshot_helper_tests {
    use super::{parse_snapshot_name, snapshot_name};
    use vfiles_domain::SnapshotKind;

    #[test]
    fn snapshot_name_round_trip_preserves_kind_and_number() {
        let name = snapshot_name(SnapshotKind::UserCreated, 42);
        let (kind, snapshot_no) = parse_snapshot_name(&name);

        assert_eq!(kind, SnapshotKind::UserCreated);
        assert_eq!(snapshot_no, 42);
    }

    #[test]
    fn auto_snapshot_name_round_trip_preserves_kind_and_number() {
        let name = snapshot_name(SnapshotKind::AutoCommit, 7);
        let (kind, snapshot_no) = parse_snapshot_name(&name);

        assert_eq!(kind, SnapshotKind::AutoCommit);
        assert_eq!(snapshot_no, 7);
    }
}

#[async_trait::async_trait]
impl SnapshotRepo for SqliteSnapshotRepo {
    async fn create_snapshot(
        &self,
        namespace_id: &NamespaceId,
        message: Option<&str>,
        kind: SnapshotKind,
        user_id: &UserId,
    ) -> DomainResult<SnapshotId> {
        let now = time::OffsetDateTime::now_utc();
        let namespace_id_str = namespace_id.to_string();
        let user_id_str = user_id.to_string();

        for _ in 0..SNAPSHOT_NAME_RETRY_LIMIT {
            let id = SnapshotId::new();
            let snapshot_no = self.next_snapshot_no(namespace_id).await?;
            let name = snapshot_name(kind, snapshot_no);

            match sqlx::query(
                "INSERT INTO snapshots (id, namespace_id, name, description, created_at, created_by) VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(id.to_string())
            .bind(&namespace_id_str)
            .bind(name)
            .bind(message)
            .bind(now)
            .bind(&user_id_str)
            .execute(&self.pool)
            .await
            {
                Ok(_) => return Ok(id),
                Err(sqlx::Error::Database(ref db_err)) if db_err.is_unique_violation() => {
                    continue;
                }
                Err(e) => {
                    return Err(DomainError::Internal {
                        message: format!("Failed to create snapshot: {}", e),
                    });
                }
            }
        }

        Err(DomainError::Internal {
            message: "Failed to allocate a unique snapshot name after multiple retries".to_string(),
        })
    }

    async fn find_snapshot(&self, snapshot_id: &SnapshotId) -> DomainResult<Snapshot> {
        let row: (String, String, String, Option<String>, String, String) = sqlx::query_as(
            "SELECT id, namespace_id, name, description, created_at, created_by FROM snapshots WHERE id = ?"
        )
        .bind(snapshot_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DomainError::SnapshotNotFound,
            _ => DomainError::Internal { message: format!("Failed to find snapshot: {}", e) },
        })?;
        let (kind, snapshot_no) = parse_snapshot_name(&row.2);

        Ok(Snapshot {
            id: SnapshotId::from_uuid(uuid::Uuid::parse_str(&row.0).map_err(|_| {
                DomainError::Internal {
                    message: "Invalid UUID".to_string(),
                }
            })?),
            namespace_id: NamespaceId::from_uuid(uuid::Uuid::parse_str(&row.1).map_err(|_| {
                DomainError::Internal {
                    message: "Invalid UUID".to_string(),
                }
            })?),
            snapshot_no,
            created_by: UserId::from_uuid(uuid::Uuid::parse_str(&row.5).map_err(|_| {
                DomainError::Internal {
                    message: "Invalid UUID".to_string(),
                }
            })?),
            created_at: time::OffsetDateTime::parse(
                &row.4,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| DomainError::Internal {
                message: "Invalid timestamp".to_string(),
            })?,
            message: row
                .3
                .as_deref()
                .and_then(|value| NonEmptyMessage::new(value).ok()),
            kind,
        })
    }

    async fn list_snapshots(
        &self,
        namespace_id: &NamespaceId,
        limit: u32,
        _cursor: Option<&str>,
    ) -> DomainResult<Vec<Snapshot>> {
        let rows: Vec<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT id, namespace_id, name, description, created_at, created_by FROM snapshots WHERE namespace_id = ? ORDER BY created_at DESC LIMIT ?"
        )
        .bind(namespace_id.to_string())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list snapshots: {}", e),
        })?;

        let mut snapshots = Vec::new();
        for row in rows {
            let (kind, snapshot_no) = parse_snapshot_name(&row.2);

            snapshots.push(Snapshot {
                id: SnapshotId::from_uuid(uuid::Uuid::parse_str(&row.0).map_err(|_| {
                    DomainError::Internal {
                        message: "Invalid UUID".to_string(),
                    }
                })?),
                namespace_id: NamespaceId::from_uuid(uuid::Uuid::parse_str(&row.1).map_err(
                    |_| DomainError::Internal {
                        message: "Invalid UUID".to_string(),
                    },
                )?),
                snapshot_no,
                created_by: UserId::from_uuid(uuid::Uuid::parse_str(&row.5).map_err(|_| {
                    DomainError::Internal {
                        message: "Invalid UUID".to_string(),
                    }
                })?),
                created_at: time::OffsetDateTime::parse(
                    &row.4,
                    &time::format_description::well_known::Rfc3339,
                )
                .map_err(|_| DomainError::Internal {
                    message: "Invalid timestamp".to_string(),
                })?,
                message: row
                    .3
                    .as_deref()
                    .and_then(|value| NonEmptyMessage::new(value).ok()),
                kind,
            });
        }

        Ok(snapshots)
    }

    async fn add_snapshot_entries(
        &self,
        snapshot_id: &SnapshotId,
        entries: &[SnapshotEntry],
    ) -> DomainResult<()> {
        let mut tx = self.pool.begin().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to begin snapshot entry transaction: {}", e),
        })?;

        for entry in entries {
            sqlx::query(
                r#"
                INSERT INTO snapshot_entries (
                    snapshot_id,
                    entry_id,
                    entry_version_id,
                    entry_path,
                    entry_kind,
                    blob_id,
                    size,
                    content_type,
                    version_no,
                    change_type,
                    created_by,
                    created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(snapshot_id.to_string())
            .bind(entry.entry_id.to_string())
            .bind(entry.entry_version_id.map(|value| value.to_string()))
            .bind(entry.entry_path.as_str())
            .bind(entry_kind_as_str(entry.entry_kind))
            .bind(entry.blob_id.map(|value| value.to_string()))
            .bind(entry.size_bytes.map(|value| value.as_u64() as i64))
            .bind(entry.mime_type.as_deref())
            .bind(entry.version_no.map(i64::from))
            .bind(change_type_as_str(entry.change_type))
            .bind(entry.created_by.map(|value| value.to_string()))
            .bind(entry.created_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to add snapshot entry: {}", e),
            })?;

            if let Some(blob_id) = entry.blob_id {
                sqlx::query("UPDATE blobs SET ref_count = ref_count + 1 WHERE id = ?")
                    .bind(blob_id.to_string())
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to update blob reference count: {}", e),
                    })?;
            }
        }

        tx.commit().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to commit snapshot entry transaction: {}", e),
        })?;

        Ok(())
    }

    async fn get_snapshot_entries(
        &self,
        snapshot_id: &SnapshotId,
    ) -> DomainResult<Vec<SnapshotEntry>> {
        let rows: Vec<(
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<i64>,
            String,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
            SELECT
                snapshot_id,
                entry_id,
                entry_version_id,
                entry_path,
                entry_kind,
                blob_id,
                size,
                content_type,
                version_no,
                change_type,
                created_by,
                created_at
            FROM snapshot_entries
            WHERE snapshot_id = ?
            ORDER BY entry_path ASC
            "#,
        )
        .bind(snapshot_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to get snapshot entries: {}", e),
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(SnapshotEntry {
                snapshot_id: SnapshotId::from_uuid(uuid::Uuid::parse_str(&row.0).map_err(
                    |_| DomainError::Internal {
                        message: "Invalid UUID".to_string(),
                    },
                )?),
                entry_id: EntryId::from_uuid(uuid::Uuid::parse_str(&row.1).map_err(|_| {
                    DomainError::Internal {
                        message: "Invalid UUID".to_string(),
                    }
                })?),
                entry_path: NormalizedPath::new(&row.3).map_err(|_| DomainError::Internal {
                    message: "Invalid path".to_string(),
                })?,
                entry_kind: parse_entry_kind(&row.4)?,
                entry_version_id: parse_version_id_opt(row.2.as_deref())?,
                blob_id: parse_blob_id_opt(row.5.as_deref())?,
                size_bytes: row.6.map(|value| ByteSize::new(value as u64)),
                mime_type: row.7,
                version_no: row.8.map(|value| value as u32),
                change_type: parse_change_type(&row.9)?,
                created_by: parse_user_id_opt(row.10.as_deref())?,
                created_at: parse_timestamp_opt(row.11.as_deref())?,
            });
        }

        Ok(entries)
    }

    async fn list_all_snapshots(&self) -> DomainResult<Vec<Snapshot>> {
        let rows: Vec<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT id, namespace_id, name, description, created_at, created_by \
             FROM snapshots ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list all snapshots: {}", e),
        })?;

        let mut snapshots = Vec::new();
        for row in rows {
            let (kind, snapshot_no) = parse_snapshot_name(&row.2);
            snapshots.push(Snapshot {
                id: SnapshotId::from_uuid(uuid::Uuid::parse_str(&row.0).map_err(|_| {
                    DomainError::Internal {
                        message: "Invalid UUID".to_string(),
                    }
                })?),
                namespace_id: NamespaceId::from_uuid(uuid::Uuid::parse_str(&row.1).map_err(
                    |_| DomainError::Internal {
                        message: "Invalid UUID".to_string(),
                    },
                )?),
                snapshot_no,
                created_by: UserId::from_uuid(uuid::Uuid::parse_str(&row.5).map_err(|_| {
                    DomainError::Internal {
                        message: "Invalid UUID".to_string(),
                    }
                })?),
                created_at: parse_timestamp(&row.4)?,
                message: row
                    .3
                    .as_deref()
                    .and_then(|value| NonEmptyMessage::new(value).ok()),
                kind,
            });
        }

        Ok(snapshots)
    }

    async fn delete_snapshot(&self, snapshot_id: &SnapshotId) -> DomainResult<()> {
        // snapshot_entries 通过外键 ON DELETE CASCADE 一并删除。
        sqlx::query("DELETE FROM snapshots WHERE id = ?")
            .bind(snapshot_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to delete snapshot: {}", e),
            })?;
        Ok(())
    }
}

use camino::Utf8PathBuf;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone)]
pub struct FsBlobStore {
    pool: SqlitePool,
    base_path: Utf8PathBuf,
}

impl FsBlobStore {
    pub fn new(pool: SqlitePool, base_path: Utf8PathBuf) -> Self {
        Self { pool, base_path }
    }

    fn blob_id_for_hash(hash_hex: &str) -> BlobId {
        let hash_bytes = hex::decode(hash_hex).expect("content hash should decode");
        let mut uuid_bytes = [0_u8; 16];
        uuid_bytes.copy_from_slice(&hash_bytes[..16]);
        uuid_bytes[6] = (uuid_bytes[6] & 0x0f) | 0x50;
        uuid_bytes[8] = (uuid_bytes[8] & 0x3f) | 0x80;
        BlobId::from_uuid(uuid::Uuid::from_bytes(uuid_bytes))
    }

    fn get_blob_path(&self, blob_id: &BlobId) -> Utf8PathBuf {
        let hash = blob_id.to_string();
        // Use first 2 chars as directory, rest as filename
        let dir = &hash[0..2];
        let filename = &hash[2..];
        self.base_path.join(dir).join(filename)
    }
}

#[async_trait::async_trait]
impl BlobStore for FsBlobStore {
    async fn store_blob(
        &self,
        data: &[u8],
        expected_sha256: Option<&str>,
    ) -> DomainResult<(BlobId, ContentHash, bool)> {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash_bytes = hasher.finalize();
        let hash_hex = hex::encode(hash_bytes);
        let content_hash = ContentHash::new(&hash_hex).map_err(|_| DomainError::Internal {
            message: "Invalid hash format".to_string(),
        })?;

        if let Some(expected) = expected_sha256
            && expected != hash_hex
        {
            return Err(DomainError::Internal {
                message: format!(
                    "Blob integrity check failed: expected {}, got {}",
                    expected, hash_hex
                ),
            });
        }

        let blob_id = Self::blob_id_for_hash(&hash_hex);
        let blob_path = self.get_blob_path(&blob_id);

        if fs::try_exists(&blob_path)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to inspect blob path: {}", e),
            })?
        {
            return Ok((blob_id, content_hash, false));
        }

        // Create directory if it doesn't exist
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to create blob directory: {}", e),
                })?;
        }

        // Write blob data
        let mut file = fs::File::create(&blob_path)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to create blob file: {}", e),
            })?;
        file.write_all(data)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to write blob data: {}", e),
            })?;
        file.flush().await.map_err(|e| DomainError::Internal {
            message: format!("Failed to flush blob file: {}", e),
        })?;

        Ok((blob_id, content_hash, true))
    }

    async fn store_blob_stream(
        &self,
        mut reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
        expected_sha256: Option<&str>,
    ) -> DomainResult<(BlobId, ContentHash, bool, u64)> {
        let temp_dir = self.base_path.join("tmp");
        fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to create blob temp directory: {}", e),
            })?;
        let temp_path = temp_dir.join(format!("blob-upload-{}.tmp", uuid::Uuid::new_v4()));
        let mut temp_file =
            fs::File::create(&temp_path)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to create blob temp file: {}", e),
                })?;
        let mut hasher = Sha256::new();
        let mut total_size = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];

        // 读取/写入中途失败（例如客户端断开）必须删除临时文件，否则会一直留在
        // tmp/ 里。把复制过程收敛为一个结果，失败时统一清理后再返回错误。
        let copy_result: Result<(), DomainError> = async {
            loop {
                let read = reader
                    .read(&mut buffer)
                    .await
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to read blob stream: {}", e),
                    })?;
                if read == 0 {
                    break;
                }

                hasher.update(&buffer[..read]);
                total_size += read as u64;
                temp_file
                    .write_all(&buffer[..read])
                    .await
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to write blob temp file: {}", e),
                    })?;
            }

            temp_file.flush().await.map_err(|e| DomainError::Internal {
                message: format!("Failed to flush blob temp file: {}", e),
            })?;

            Ok(())
        }
        .await;

        drop(temp_file);

        if let Err(err) = copy_result {
            let _ = fs::remove_file(&temp_path).await;
            return Err(err);
        }

        let hash_hex = hex::encode(hasher.finalize());
        let content_hash = ContentHash::new(&hash_hex).map_err(|_| DomainError::Internal {
            message: "Invalid hash format".to_string(),
        })?;

        if let Some(expected) = expected_sha256
            && expected != hash_hex
        {
            let _ = fs::remove_file(&temp_path).await;
            return Err(DomainError::Internal {
                message: format!(
                    "Blob integrity check failed: expected {}, got {}",
                    expected, hash_hex
                ),
            });
        }

        let blob_id = Self::blob_id_for_hash(&hash_hex);
        let blob_path = self.get_blob_path(&blob_id);

        if fs::try_exists(&blob_path)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to inspect blob path: {}", e),
            })?
        {
            let _ = fs::remove_file(&temp_path).await;
            return Ok((blob_id, content_hash, false, total_size));
        }

        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to create blob directory: {}", e),
                })?;
        }

        match fs::rename(&temp_path, &blob_path).await {
            Ok(()) => Ok((blob_id, content_hash, true, total_size)),
            Err(e) => {
                if fs::try_exists(&blob_path).await.map_err(|inspect_err| {
                    DomainError::Internal {
                        message: format!(
                            "Failed to inspect blob path after rename failure: {}",
                            inspect_err
                        ),
                    }
                })? {
                    let _ = fs::remove_file(&temp_path).await;
                    Ok((blob_id, content_hash, false, total_size))
                } else {
                    Err(DomainError::Internal {
                        message: format!("Failed to move blob temp file into place: {}", e),
                    })
                }
            }
        }
    }

    async fn get_blob(&self, blob_id: &BlobId) -> DomainResult<Option<Vec<u8>>> {
        let blob_path = self.get_blob_path(blob_id);
        match fs::read(&blob_path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(DomainError::Internal {
                message: format!("Failed to read blob: {}", e),
            }),
        }
    }

    async fn get_blob_stream(
        &self,
        blob_id: &BlobId,
    ) -> DomainResult<Option<Box<dyn vfiles_domain::ReadSeek + Send + Unpin>>> {
        let blob_path = self.get_blob_path(blob_id);
        match fs::File::open(&blob_path).await {
            Ok(file) => Ok(Some(Box::new(file))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(DomainError::Internal {
                message: format!("Failed to open blob stream: {}", e),
            }),
        }
    }

    async fn delete_blob(&self, blob_id: &BlobId) -> DomainResult<()> {
        let blob_path = self.get_blob_path(blob_id);
        match fs::remove_file(&blob_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // Already deleted
            Err(e) => Err(DomainError::Internal {
                message: format!("Failed to delete blob: {}", e),
            }),
        }
    }

    async fn blob_exists(&self, sha256: &ContentHash) -> DomainResult<bool> {
        let blob_id = Self::blob_id_for_hash(sha256.as_str());
        fs::try_exists(self.get_blob_path(&blob_id))
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to inspect blob path: {}", e),
            })
    }

    async fn get_blob_metadata(&self, blob_id: &BlobId) -> DomainResult<Option<Blob>> {
        let row: Option<(String, String, i64, Option<String>, i64, String, Option<String>)> =
            sqlx::query_as(
                r#"
                SELECT content_hash, storage_key, size, content_type, ref_count, created_at, verified_at
                FROM blobs
                WHERE id = ?
                LIMIT 1
                "#,
            )
            .bind(blob_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to get blob metadata: {}", e),
            })?;

        row.map(
            |(
                content_hash,
                storage_key,
                size,
                content_type,
                ref_count,
                created_at,
                verified_at,
            )| {
                Ok(Blob {
                    id: *blob_id,
                    sha256_hex: content_hash,
                    storage_key,
                    size_bytes: ByteSize::new(size as u64),
                    mime_type_detected: content_type,
                    ref_count: ref_count.max(0) as u32,
                    created_at: parse_timestamp(&created_at)?,
                    verified_at: verified_at.as_deref().map(parse_timestamp).transpose()?,
                })
            },
        )
        .transpose()
    }

    async fn list_blobs(&self) -> DomainResult<Vec<Blob>> {
        let rows: Vec<(
            String,
            String,
            String,
            i64,
            Option<String>,
            i64,
            String,
            Option<String>,
        )> = sqlx::query_as(
            r#"
            SELECT id, content_hash, storage_key, size, content_type, ref_count, created_at, verified_at
            FROM blobs
            ORDER BY created_at
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list blobs: {}", e),
        })?;

        rows.into_iter()
            .map(
                |(
                    id,
                    content_hash,
                    storage_key,
                    size,
                    content_type,
                    ref_count,
                    created_at,
                    verified_at,
                )| {
                    Ok(Blob {
                        id: BlobId::from_string(&id).map_err(|_| DomainError::Internal {
                            message: "Invalid blob id".to_string(),
                        })?,
                        sha256_hex: content_hash,
                        storage_key,
                        size_bytes: ByteSize::new(size as u64),
                        mime_type_detected: content_type,
                        ref_count: ref_count.max(0) as u32,
                        created_at: parse_timestamp(&created_at)?,
                        verified_at: verified_at.as_deref().map(parse_timestamp).transpose()?,
                    })
                },
            )
            .collect()
    }

    async fn purge_blob(&self, blob_id: &BlobId, expected_ref_count: u32) -> DomainResult<bool> {
        // 乐观校验：计数被并发修改时跳过，避免误删正在使用的 blob。
        let result = sqlx::query("DELETE FROM blobs WHERE id = ? AND ref_count = ?")
            .bind(blob_id.to_string())
            .bind(i64::from(expected_ref_count))
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to purge blob row: {}", e),
            })?;

        if result.rows_affected() == 0 {
            return Ok(false);
        }

        self.delete_blob(blob_id).await?;
        Ok(true)
    }

    async fn list_stored_blob_files(
        &self,
    ) -> DomainResult<Vec<(BlobId, time::OffsetDateTime, u64)>> {
        // 目录结构为 <base>/<前2位>/<其余36-2位>，由文件名可还原 BlobId。
        let mut files = Vec::new();
        let base = &self.base_path;
        if !base.is_dir() {
            return Ok(files);
        }

        let mut shard_dirs = fs::read_dir(base)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to read blob directory: {}", e),
            })?;

        while let Some(entry) =
            shard_dirs
                .next_entry()
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to read blob directory entry: {}", e),
                })?
        {
            let shard = entry.file_name().to_string_lossy().into_owned();
            if shard.len() != 2 {
                continue;
            }

            let mut shard_files = match fs::read_dir(entry.path()).await {
                Ok(files) => files,
                Err(_) => continue,
            };

            while let Some(file_entry) =
                shard_files
                    .next_entry()
                    .await
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to read blob file entry: {}", e),
                    })?
            {
                let name = file_entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(".tmp") {
                    continue;
                }

                let candidate = format!("{shard}{name}");
                let Ok(blob_id) = BlobId::from_string(&candidate) else {
                    continue;
                };

                let Ok(metadata) = file_entry.metadata().await else {
                    continue;
                };
                let modified = metadata
                    .modified()
                    .map(time::OffsetDateTime::from)
                    .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
                files.push((blob_id, modified, metadata.len()));
            }
        }

        Ok(files)
    }
}

#[derive(Debug)]
pub struct FsUploadStore {
    base_path: Utf8PathBuf,
}

impl FsUploadStore {
    pub fn new(base_path: Utf8PathBuf) -> Self {
        Self { base_path }
    }

    fn assembled_upload_path(&self, upload_id: &UploadId) -> Utf8PathBuf {
        self.base_path
            .join(upload_id.to_string())
            .join("assembled.bin")
    }

    fn get_upload_path(&self, upload_id: &UploadId, part_index: Option<u32>) -> Utf8PathBuf {
        let upload_dir = self.base_path.join(upload_id.to_string());
        match part_index {
            Some(index) => upload_dir.join(format!("part_{}", index)),
            None => upload_dir.join("metadata.json"),
        }
    }

    async fn read_metadata(&self, upload_id: &UploadId) -> DomainResult<serde_json::Value> {
        let metadata_path = self.get_upload_path(upload_id, None);
        let metadata_json = fs::read_to_string(&metadata_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DomainError::NotFound {
                    resource: "upload session".to_string(),
                }
            } else {
                DomainError::Internal {
                    message: format!("Failed to read metadata: {}", e),
                }
            }
        })?;

        serde_json::from_str(&metadata_json).map_err(|e| DomainError::Internal {
            message: format!("Failed to parse metadata: {}", e),
        })
    }

    async fn write_metadata(
        &self,
        upload_id: &UploadId,
        metadata: &serde_json::Value,
    ) -> DomainResult<()> {
        let metadata_path = self.get_upload_path(upload_id, None);
        let metadata_json =
            serde_json::to_string_pretty(metadata).map_err(|e| DomainError::Internal {
                message: format!("Failed to serialize metadata: {}", e),
            })?;

        fs::write(&metadata_path, metadata_json)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to write metadata: {}", e),
            })
    }

    fn parse_timestamp(
        value: &serde_json::Value,
        field: &str,
    ) -> DomainResult<time::OffsetDateTime> {
        let raw = value.as_str().ok_or_else(|| DomainError::Internal {
            message: format!("Missing upload metadata field: {}", field),
        })?;

        time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc3339).map_err(
            |e| DomainError::Internal {
                message: format!("Invalid upload metadata timestamp {}: {}", field, e),
            },
        )
    }

    fn parse_state(value: Option<&str>) -> UploadState {
        match value.unwrap_or("receiving") {
            "assembling" => UploadState::Assembling,
            "completed" => UploadState::Completed,
            "failed" => UploadState::Failed,
            _ => UploadState::Receiving,
        }
    }
}

impl Clone for FsUploadStore {
    fn clone(&self) -> Self {
        Self {
            base_path: self.base_path.clone(),
        }
    }
}

#[async_trait::async_trait]
impl UploadStore for FsUploadStore {
    async fn create_upload_session(
        &self,
        namespace_id: &NamespaceId,
        target_path: &NormalizedPath,
        filename: &str,
        size_bytes: u64,
        chunk_size: u64,
        user_id: &UserId,
    ) -> DomainResult<UploadId> {
        let upload_id = UploadId::new();
        let upload_dir = self.base_path.join(upload_id.to_string());
        let now = time::OffsetDateTime::now_utc();
        let expires_at = now + time::Duration::hours(24);
        let total_chunks = if chunk_size == 0 {
            0
        } else {
            size_bytes.div_ceil(chunk_size) as u32
        };

        // Create upload directory
        fs::create_dir_all(&upload_dir)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to create upload directory: {}", e),
            })?;

        // Store metadata
        let metadata = serde_json::json!({
            "upload_id": upload_id.to_string(),
            "namespace_id": namespace_id.to_string(),
            "target_path": target_path.as_str(),
            "filename": filename,
            "size_bytes": size_bytes,
            "chunk_size": chunk_size,
            "total_chunks": total_chunks,
            "user_id": user_id.to_string(),
            "created_at": now.format(&time::format_description::well_known::Rfc3339).map_err(|e| DomainError::Internal {
                message: format!("Failed to format upload creation timestamp: {}", e),
            })?,
            "updated_at": now.format(&time::format_description::well_known::Rfc3339).map_err(|e| DomainError::Internal {
                message: format!("Failed to format upload update timestamp: {}", e),
            })?,
            "expires_at": expires_at.format(&time::format_description::well_known::Rfc3339).map_err(|e| DomainError::Internal {
                message: format!("Failed to format upload expiry timestamp: {}", e),
            })?,
            "status": "receiving"
        });

        let metadata_path = self.get_upload_path(&upload_id, None);
        let metadata_json =
            serde_json::to_string_pretty(&metadata).map_err(|e| DomainError::Internal {
                message: format!("Failed to serialize metadata: {}", e),
            })?;

        fs::write(&metadata_path, metadata_json)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to write metadata: {}", e),
            })?;

        Ok(upload_id)
    }

    async fn get_upload_session(&self, upload_id: &UploadId) -> DomainResult<UploadSession> {
        let metadata = self.read_metadata(upload_id).await?;
        let created_at = Self::parse_timestamp(&metadata["created_at"], "created_at")?;
        let updated_at = Self::parse_timestamp(&metadata["updated_at"], "updated_at")?;
        let expires_at = Self::parse_timestamp(&metadata["expires_at"], "expires_at")?;
        let completed_at = metadata
            .get("completed_at")
            .filter(|value| !value.is_null())
            .map(|value| Self::parse_timestamp(value, "completed_at"))
            .transpose()?;

        Ok(UploadSession {
            id: *upload_id,
            namespace_id: NamespaceId::from_uuid(
                uuid::Uuid::parse_str(metadata["namespace_id"].as_str().unwrap_or("")).map_err(
                    |_| DomainError::Internal {
                        message: "Invalid namespace ID".to_string(),
                    },
                )?,
            ),
            target_path_norm: NormalizedPath::new(metadata["target_path"].as_str().unwrap_or(""))
                .map_err(|_| DomainError::Internal {
                message: "Invalid path".to_string(),
            })?,
            filename: metadata["filename"].as_str().unwrap_or("").to_string(),
            declared_size: ByteSize::new(metadata["size_bytes"].as_u64().unwrap_or(0)),
            chunk_size: metadata["chunk_size"].as_u64().unwrap_or(0),
            total_chunks: metadata["total_chunks"].as_u64().unwrap_or(0) as u32,
            state: Self::parse_state(metadata["status"].as_str()),
            owner_user_id: UserId::from_uuid(
                uuid::Uuid::parse_str(metadata["user_id"].as_str().unwrap_or("")).map_err(
                    |_| DomainError::Internal {
                        message: "Invalid user ID".to_string(),
                    },
                )?,
            ),
            expires_at,
            created_at,
            updated_at,
            completed_at,
        })
    }

    async fn store_upload_part(
        &self,
        upload_id: &UploadId,
        part_index: u32,
        data: &[u8],
        sha256: &str,
    ) -> DomainResult<()> {
        let part_path = self.get_upload_path(upload_id, Some(part_index));
        let now = time::OffsetDateTime::now_utc();

        // Verify hash
        let mut hasher = Sha256::new();
        hasher.update(data);
        let actual_hash = hex::encode(hasher.finalize());
        if actual_hash != sha256 {
            return Err(DomainError::UploadPartInvalid);
        }

        fs::write(&part_path, data)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to write upload part: {}", e),
            })?;

        let mut metadata = self.read_metadata(upload_id).await?;
        metadata["updated_at"] = now
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to format upload update timestamp: {}", e),
            })?
            .into();
        self.write_metadata(upload_id, &metadata).await?;

        Ok(())
    }

    async fn get_upload_parts(&self, upload_id: &UploadId) -> DomainResult<Vec<UploadPart>> {
        let upload_dir = self.base_path.join(upload_id.to_string());
        let mut parts = Vec::new();

        // Read all part files
        let mut entries = fs::read_dir(&upload_dir)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to read upload directory: {}", e),
            })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to read directory entry: {}", e),
            })?
        {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                && filename.starts_with("part_")
                && let Ok(part_index) = filename.trim_start_matches("part_").parse::<u32>()
            {
                let metadata = entry.metadata().await.map_err(|e| DomainError::Internal {
                    message: format!("Failed to get part metadata: {}", e),
                })?;
                let size_bytes = ByteSize::new(metadata.len());

                parts.push(UploadPart {
                    upload_session_id: *upload_id,
                    part_index,
                    temp_rel_path: format!("part_{}", part_index),
                    size_bytes,
                    sha256_hex: String::new(), // Would need to be stored
                    received_at: time::OffsetDateTime::now_utc(),
                });
            }
        }

        parts.sort_by_key(|p| p.part_index);
        Ok(parts)
    }

    async fn assemble_upload_stream(
        &self,
        upload_id: &UploadId,
    ) -> DomainResult<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
        let session = self.get_upload_session(upload_id).await?;
        let parts = self.get_upload_parts(upload_id).await?;

        if session.total_chunks > 0 && parts.len() != session.total_chunks as usize {
            return Err(DomainError::UploadConflict);
        }

        let assembled_path = self.assembled_upload_path(upload_id);
        if let Some(parent) = assembled_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to create assembled upload directory: {}", e),
                })?;
        }

        let _ = fs::remove_file(&assembled_path).await;
        let mut assembled_file =
            fs::File::create(&assembled_path)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to create assembled upload file: {}", e),
                })?;

        for expected_index in 0..parts.len() {
            let part = parts
                .get(expected_index)
                .ok_or(DomainError::UploadConflict)?;
            if part.part_index as usize != expected_index {
                return Err(DomainError::UploadConflict);
            }

            let part_path = self.get_upload_path(upload_id, Some(part.part_index));
            let mut part_file =
                fs::File::open(&part_path)
                    .await
                    .map_err(|e| DomainError::Internal {
                        message: format!("Failed to open upload part {}: {}", part.part_index, e),
                    })?;
            tokio::io::copy(&mut part_file, &mut assembled_file)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to append upload part {}: {}", part.part_index, e),
                })?;
        }

        assembled_file
            .flush()
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to flush assembled upload file: {}", e),
            })?;
        drop(assembled_file);

        let assembled_reader =
            fs::File::open(&assembled_path)
                .await
                .map_err(|e| DomainError::Internal {
                    message: format!("Failed to reopen assembled upload file: {}", e),
                })?;
        Ok(Box::new(assembled_reader))
    }

    async fn assemble_upload(&self, upload_id: &UploadId) -> DomainResult<Vec<u8>> {
        let session = self.get_upload_session(upload_id).await?;
        let mut reader = self.assemble_upload_stream(upload_id).await?;
        let mut assembled = Vec::with_capacity(session.declared_size.as_u64() as usize);
        reader
            .read_to_end(&mut assembled)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to read assembled upload stream: {}", e),
            })?;
        Ok(assembled)
    }

    async fn complete_upload_session(&self, upload_id: &UploadId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        let mut metadata = self.read_metadata(upload_id).await?;
        metadata["status"] = "completed".into();
        metadata["updated_at"] = now
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to format upload update timestamp: {}", e),
            })?
            .into();
        metadata["completed_at"] = now
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to format upload completion timestamp: {}", e),
            })?
            .into();

        self.write_metadata(upload_id, &metadata).await?;

        let upload_dir = self.base_path.join(upload_id.to_string());
        let mut entries = fs::read_dir(&upload_dir)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to inspect upload directory for cleanup: {}", e),
            })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to read upload cleanup entry: {}", e),
            })?
        {
            let Some(file_name) = entry.file_name().to_str().map(|value| value.to_string()) else {
                continue;
            };

            if file_name.starts_with("part_") || file_name == "assembled.bin" {
                let _ = fs::remove_file(entry.path()).await;
            }
        }

        Ok(())
    }

    async fn cancel_upload_session(&self, upload_id: &UploadId) -> DomainResult<()> {
        let upload_dir = self.base_path.join(upload_id.to_string());

        // Remove entire upload directory
        match fs::remove_dir_all(&upload_dir).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DomainError::Internal {
                message: format!("Failed to cancel upload: {}", e),
            }),
        }
    }

    async fn cleanup_expired_sessions(&self) -> DomainResult<i64> {
        // Simplified implementation - would need to check metadata timestamps
        Ok(0)
    }
}

// Search repository implementation
#[derive(Debug)]
pub struct SqliteSearchRepo<B> {
    pool: SqlitePool,
    blob_store: B,
}

impl<B> SqliteSearchRepo<B> {
    pub fn new(pool: SqlitePool, blob_store: B) -> Self {
        Self { pool, blob_store }
    }
}

impl<B> Clone for SqliteSearchRepo<B>
where
    B: Clone,
{
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            blob_store: self.blob_store.clone(),
        }
    }
}

/// 文件名搜索最多取回多少行候选（分页在应用层完成，这里只做兜底）。
const MAX_FILENAME_SEARCH_CANDIDATES: i64 = 2000;
/// 内容搜索最多扫描多少个候选文件。
const MAX_CONTENT_SEARCH_CANDIDATES: i64 = 500;
/// 内容搜索最多返回多少条命中。
const MAX_CONTENT_SEARCH_MATCHES: usize = 500;

#[async_trait::async_trait]
impl<B> SearchRepo for SqliteSearchRepo<B>
where
    B: BlobStore + Send + Sync,
{
    async fn search_entries(&self, query: &SearchQuery) -> DomainResult<Vec<SearchResult>> {
        if !query.search_files {
            return Ok(vec![]);
        }

        let search_pattern = format!("%{}%", query.query.to_lowercase());
        let namespace_id = query.namespace_id.to_string();
        let entry_kind = query.entry_kind.map(|kind| match kind {
            EntryKind::File => "file".to_string(),
            EntryKind::Directory => "directory".to_string(),
        });
        let path_exact = query
            .path_prefix
            .as_ref()
            .map(|path| path.as_str().to_string());
        let path_like = path_exact.as_ref().map(|path| format!("{path}/%"));
        // 分页由应用层在按得分排序后统一处理：这里返回候选范围内的全部文件名命中，
        // 否则「下一页」会按 created_at 而不是最终得分排序，页与页之间会出现重复/遗漏。
        let candidate_limit = MAX_FILENAME_SEARCH_CANDIDATES;

        #[derive(sqlx::FromRow)]
        struct EntrySearchRow {
            entry_id: String,
            namespace_id: String,
            path: String,
            entry_type: String,
            entry_created_at: String,
            version_id: Option<String>,
            version_no: Option<i64>,
            blob_id: Option<String>,
            size_bytes: Option<i64>,
            mime_type: Option<String>,
            content_hash: Option<String>,
            version_created_at: Option<String>,
            created_by: Option<String>,
            change_message: Option<String>,
        }

        let rows: Vec<EntrySearchRow> = sqlx::query_as(
            r#"
            SELECT
                e.id as entry_id,
                e.namespace_id,
                e.path,
                e.kind as entry_type,
                e.created_at as entry_created_at,
                ev.id as version_id,
                ev.version as version_no,
                ev.blob_id,
                ev.size as size_bytes,
                ev.content_type as mime_type,
                                b.content_hash,
                ev.created_at as version_created_at,
                ev.created_by,
                ev.message as change_message
            FROM entries e
            LEFT JOIN entry_versions ev ON e.id = ev.entry_id
                        LEFT JOIN blobs b ON b.id = ev.blob_id
            WHERE e.namespace_id = ?
              AND (LOWER(e.path) LIKE ?)
                            AND (? IS NULL OR e.kind = ?)
                            AND (? IS NULL OR e.path = ? OR e.path LIKE ?)
            ORDER BY e.created_at DESC, e.path ASC
            LIMIT ?
            "#,
        )
        .bind(namespace_id)
        .bind(search_pattern)
        .bind(entry_kind.clone())
        .bind(entry_kind)
        .bind(path_exact.clone())
        .bind(path_exact)
        .bind(path_like)
        .bind(candidate_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to search entries: {}", e),
        })?;

        let mut results = Vec::new();
        for row in rows {
            // Extract filename from path
            let path = &row.path;
            let name = path.split('/').next_back().unwrap_or(path).to_string();

            let entry = Entry {
                id: EntryId::from_uuid(uuid::Uuid::parse_str(&row.entry_id).map_err(|e| {
                    DomainError::Internal {
                        message: format!("Invalid entry ID: {}", e),
                    }
                })?),
                namespace_id: NamespaceId::from_uuid(
                    uuid::Uuid::parse_str(&row.namespace_id).map_err(|e| {
                        DomainError::Internal {
                            message: format!("Invalid namespace ID: {}", e),
                        }
                    })?,
                ),
                parent_entry_id: None, // TODO: populate from path
                path_norm: NormalizedPath::new(path).map_err(|e| DomainError::Internal {
                    message: format!("Invalid path: {}", e),
                })?,
                name,
                entry_type: match row.entry_type.as_str() {
                    "file" => EntryKind::File,
                    "directory" => EntryKind::Directory,
                    _ => {
                        return Err(DomainError::Internal {
                            message: format!("Invalid entry type: {}", row.entry_type),
                        });
                    }
                },
                current_version_id: row.version_id.as_ref().map(|id| {
                    VersionId::from_uuid(
                        uuid::Uuid::parse_str(id)
                            .map_err(|e| DomainError::Internal {
                                message: format!("Invalid version ID: {}", e),
                            })
                            .unwrap(), // Safe because we checked
                    )
                }),
                created_at: time::OffsetDateTime::parse(
                    &row.entry_created_at,
                    &time::format_description::well_known::Rfc3339,
                )
                .unwrap_or_else(|_| time::OffsetDateTime::now_utc()),
                deleted_at: None,
            };

            let version = if let Some(version_id) = &row.version_id {
                Some(EntryVersion {
                    id: VersionId::from_uuid(uuid::Uuid::parse_str(version_id).map_err(|e| {
                        DomainError::Internal {
                            message: format!("Invalid version ID: {}", e),
                        }
                    })?),
                    entry_id: entry.id,
                    version_no: row.version_no.unwrap_or(1) as u32,
                    blob_id: row.blob_id.as_ref().map(|id| {
                        BlobId::from_uuid(
                            uuid::Uuid::parse_str(id)
                                .map_err(|e| DomainError::Internal {
                                    message: format!("Invalid blob ID: {}", e),
                                })
                                .unwrap(), // Safe because we checked
                        )
                    }),
                    size_bytes: ByteSize::new(row.size_bytes.unwrap_or(0) as u64),
                    mime_type: row.mime_type,
                    is_text: true, // TODO: determine from mime type
                    content_hash: row
                        .content_hash
                        .as_deref()
                        .map(ContentHash::new)
                        .transpose()
                        .map_err(|_| DomainError::Internal {
                            message: "Invalid content hash".to_string(),
                        })?
                        .unwrap_or_else(default_content_hash),
                    created_by: UserId::from_uuid(
                        uuid::Uuid::parse_str(row.created_by.as_ref().unwrap_or(&"".to_string()))
                            .map_err(|e| DomainError::Internal {
                            message: format!("Invalid user ID: {}", e),
                        })?,
                    ),
                    created_at: row
                        .version_created_at
                        .as_ref()
                        .map(|dt| {
                            time::OffsetDateTime::parse(
                                dt,
                                &time::format_description::well_known::Rfc3339,
                            )
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
                        })
                        .unwrap_or_else(time::OffsetDateTime::now_utc),
                    change_type: ChangeType::Modified, // Default
                    change_message: row.change_message.as_ref().map(|msg| {
                        NonEmptyMessage::new(msg)
                            .unwrap_or_else(|_| NonEmptyMessage::new("Updated").unwrap())
                    }),
                    source_upload_id: None, // TODO: populate if needed
                })
            } else {
                None
            };

            let matches = vec![SearchMatch {
                match_type: SearchMatchType::Path, // Since we're searching paths
                context: None,
                line_number: None,
            }];

            results.push(SearchResult {
                entry,
                version,
                matches,
                score: 1.0, // Simple scoring for now
            });
        }

        Ok(results)
    }

    async fn search_content(&self, query: &SearchQuery) -> DomainResult<Vec<SearchResult>> {
        if !query.search_content {
            return Ok(vec![]);
        }
        if matches!(query.entry_kind, Some(EntryKind::Directory)) {
            return Ok(vec![]);
        }

        let search_term = query.query.to_lowercase();
        let namespace_id = query.namespace_id.to_string();
        let entry_kind = query.entry_kind.map(|kind| match kind {
            EntryKind::File => "file".to_string(),
            EntryKind::Directory => "directory".to_string(),
        });
        let path_exact = query
            .path_prefix
            .as_ref()
            .map(|path| path.as_str().to_string());
        let path_like = path_exact.as_ref().map(|path| format!("{path}/%"));
        // 同文件名搜索：分页统一由应用层在排序后处理，这里只限制候选文件数量，
        // 并在候选范围内收集全部命中（上限 MAX_CONTENT_SEARCH_MATCHES）。
        let candidate_limit = MAX_CONTENT_SEARCH_CANDIDATES;

        // Define a struct for the query result
        #[derive(sqlx::FromRow)]
        struct ContentSearchRow {
            entry_id: String,
            namespace_id: String,
            path: String,
            entry_type: String,
            entry_created_at: String,
            version_id: Option<String>,
            version_no: Option<i64>,
            blob_id: Option<String>,
            size_bytes: Option<i64>,
            mime_type: Option<String>,
            content_hash: Option<String>,
            version_created_at: Option<String>,
            created_by: Option<String>,
            change_message: Option<String>,
        }

        // Find text files in the namespace
        let rows: Vec<ContentSearchRow> = sqlx::query_as::<_, ContentSearchRow>(
            r#"
            SELECT
                e.id as entry_id,
                e.namespace_id,
                e.path,
                e.kind as entry_type,
                e.created_at as entry_created_at,
                ev.id as version_id,
                ev.version as version_no,
                ev.blob_id,
                ev.size as size_bytes,
                ev.content_type as mime_type,
                b.content_hash,
                ev.created_at as version_created_at,
                ev.created_by,
                ev.message as change_message
            FROM entries e
            JOIN entry_versions ev ON e.id = ev.entry_id
            LEFT JOIN blobs b ON b.id = ev.blob_id
            WHERE e.namespace_id = ?
              AND ev.blob_id IS NOT NULL
              AND (ev.content_type LIKE 'text/%' OR ev.content_type LIKE 'application/json%')
                            AND (? IS NULL OR e.kind = ?)
                            AND (? IS NULL OR e.path = ? OR e.path LIKE ?)
            ORDER BY e.created_at DESC, e.path ASC
            LIMIT ?
            "#,
        )
        .bind(namespace_id)
        .bind(entry_kind.clone())
        .bind(entry_kind)
        .bind(path_exact.clone())
        .bind(path_exact)
        .bind(path_like)
        .bind(candidate_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to search content candidates: {}", e),
        })?;

        let mut results = Vec::new();
        for row in rows {
            // 候选范围内收集全部命中（分页在应用层完成），仍设上限防止极端耗时
            if results.len() >= MAX_CONTENT_SEARCH_MATCHES {
                break;
            }

            // Try to get blob content
            if let Some(blob_id_str) = &row.blob_id
                && let Ok(blob_id) = uuid::Uuid::parse_str(blob_id_str)
            {
                let blob_id = BlobId::from_uuid(blob_id);
                if let Ok(Some(blob_stream)) = self.blob_store.get_blob_stream(&blob_id).await {
                    let mut lines = BufReader::new(blob_stream).lines();
                    let mut line_number = 0_u32;
                    let mut matches = Vec::new();
                    let mut read_failed = false;

                    loop {
                        match lines.next_line().await {
                            Ok(Some(line)) => {
                                line_number += 1;
                                if line.to_lowercase().contains(&search_term) {
                                    matches.push(SearchMatch {
                                        match_type: SearchMatchType::Content,
                                        context: Some(line.trim().to_string()),
                                        line_number: Some(line_number),
                                    });
                                }
                            }
                            Ok(None) => break,
                            Err(_) => {
                                read_failed = true;
                                break;
                            }
                        }
                    }

                    if read_failed || matches.is_empty() {
                        continue;
                    }

                    // Extract filename from path
                    let path = &row.path;
                    let name = path.split('/').next_back().unwrap_or(path).to_string();

                    let entry = Entry {
                        id: EntryId::from_uuid(uuid::Uuid::parse_str(&row.entry_id).map_err(
                            |e| DomainError::Internal {
                                message: format!("Invalid entry ID: {}", e),
                            },
                        )?),
                        namespace_id: NamespaceId::from_uuid(
                            uuid::Uuid::parse_str(&row.namespace_id).map_err(|e| {
                                DomainError::Internal {
                                    message: format!("Invalid namespace ID: {}", e),
                                }
                            })?,
                        ),
                        parent_entry_id: None, // TODO: populate from path
                        path_norm: NormalizedPath::new(path).map_err(|e| {
                            DomainError::Internal {
                                message: format!("Invalid path: {}", e),
                            }
                        })?,
                        name,
                        entry_type: match row.entry_type.as_str() {
                            "file" => EntryKind::File,
                            "directory" => EntryKind::Directory,
                            _ => continue, // Skip invalid entries
                        },
                        current_version_id: row.version_id.as_ref().map(|id| {
                            VersionId::from_uuid(
                                uuid::Uuid::parse_str(id)
                                    .map_err(|e| DomainError::Internal {
                                        message: format!("Invalid version ID: {}", e),
                                    })
                                    .unwrap(), // Safe because we checked
                            )
                        }),
                        created_at: time::OffsetDateTime::parse(
                            &row.entry_created_at,
                            &time::format_description::well_known::Rfc3339,
                        )
                        .unwrap_or_else(|_| time::OffsetDateTime::now_utc()),
                        deleted_at: None,
                    };

                    let version = if let Some(version_id) = &row.version_id {
                        Some(EntryVersion {
                            id: VersionId::from_uuid(uuid::Uuid::parse_str(version_id).map_err(
                                |e| DomainError::Internal {
                                    message: format!("Invalid version ID: {}", e),
                                },
                            )?),
                            entry_id: entry.id,
                            version_no: row.version_no.unwrap_or(1) as u32,
                            blob_id: Some(blob_id),
                            size_bytes: ByteSize::new(row.size_bytes.unwrap_or(0) as u64),
                            mime_type: row.mime_type,
                            is_text: true,
                            content_hash: row
                                .content_hash
                                .as_deref()
                                .map(ContentHash::new)
                                .transpose()
                                .map_err(|_| DomainError::Internal {
                                    message: "Invalid content hash".to_string(),
                                })?
                                .unwrap_or_else(default_content_hash),
                            created_by: UserId::from_uuid(
                                uuid::Uuid::parse_str(
                                    row.created_by.as_ref().unwrap_or(&"".to_string()),
                                )
                                .map_err(|e| {
                                    DomainError::Internal {
                                        message: format!("Invalid user ID: {}", e),
                                    }
                                })?,
                            ),
                            created_at: row
                                .version_created_at
                                .as_ref()
                                .map(|dt| {
                                    time::OffsetDateTime::parse(
                                        dt,
                                        &time::format_description::well_known::Rfc3339,
                                    )
                                    .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
                                })
                                .unwrap_or_else(time::OffsetDateTime::now_utc),
                            change_type: ChangeType::Modified,
                            change_message: row.change_message.as_ref().map(|msg| {
                                NonEmptyMessage::new(msg)
                                    .unwrap_or_else(|_| NonEmptyMessage::new("Updated").unwrap())
                            }),
                            source_upload_id: None,
                        })
                    } else {
                        None
                    };

                    results.push(SearchResult {
                        entry,
                        version,
                        matches,
                        score: 0.8, // Content matches get slightly lower score than filename matches
                    });
                }
            }
        }

        Ok(results)
    }
}

#[derive(Debug)]
pub struct SqliteShareRepo {
    pool: SqlitePool,
}

impl SqliteShareRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl Clone for SqliteShareRepo {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

type ShareRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
);

/// `entries` 表只有路径没有独立名称列，展示用的名称从路径末段推导。
fn entry_name_from_path(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn parse_share_row(
    (
        id,
        namespace_id,
        entry_id,
        entry_version_id,
        code,
        expires_at,
        created_by,
        created_at,
        access_count,
        last_accessed_at,
        disabled_at,
    ): ShareRow,
) -> DomainResult<Share> {
    Ok(Share {
        id: ShareId::from_uuid(
            uuid::Uuid::parse_str(&id).map_err(|e| DomainError::Internal {
                message: format!("Invalid share ID: {}", e),
            })?,
        ),
        namespace_id: NamespaceId::from_uuid(uuid::Uuid::parse_str(&namespace_id).map_err(
            |e| DomainError::Internal {
                message: format!("Invalid namespace ID: {}", e),
            },
        )?),
        entry_id: EntryId::from_uuid(uuid::Uuid::parse_str(&entry_id).map_err(|e| {
            DomainError::Internal {
                message: format!("Invalid entry ID: {}", e),
            }
        })?),
        entry_version_id: parse_version_id_opt(entry_version_id.as_deref())?,
        code,
        expires_at: parse_timestamp_opt(expires_at.as_deref())?,
        created_by: UserId::from_uuid(uuid::Uuid::parse_str(&created_by).map_err(|e| {
            DomainError::Internal {
                message: format!("Invalid user ID: {}", e),
            }
        })?),
        created_at: parse_timestamp(&created_at)?,
        access_count: access_count as u32,
        last_accessed_at: parse_timestamp_opt(last_accessed_at.as_deref())?,
        disabled_at: parse_timestamp_opt(disabled_at.as_deref())?,
    })
}

#[async_trait::async_trait]
impl ShareRepo for SqliteShareRepo {
    async fn create_share(
        &self,
        namespace_id: &NamespaceId,
        entry_id: &EntryId,
        entry_version_id: Option<&VersionId>,
        code: &str,
        expires_at: Option<time::OffsetDateTime>,
        created_by: &UserId,
    ) -> DomainResult<ShareId> {
        let id = ShareId::new();
        let now = time::OffsetDateTime::now_utc();

        sqlx::query(
            r#"
            INSERT INTO shares (id, namespace_id, entry_id, entry_version_id, code, expires_at, created_by, created_at, access_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)
            "#,
        )
        .bind(id.to_string())
        .bind(namespace_id.to_string())
        .bind(entry_id.to_string())
        .bind(entry_version_id.map(|id| id.to_string()))
        .bind(code)
        .bind(expires_at)
        .bind(created_by.to_string())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to create share: {}", e),
        })?;

        Ok(id)
    }

    async fn find_share_by_code(&self, code: &str) -> DomainResult<Share> {
        let now = time::OffsetDateTime::now_utc();
        let row: ShareRow = sqlx::query_as(
            r#"
            SELECT
                id, namespace_id, entry_id, entry_version_id, code, expires_at,
                created_by, created_at, access_count, last_accessed_at, disabled_at
            FROM shares
            WHERE code = ? AND (expires_at IS NULL OR expires_at > ?) AND disabled_at IS NULL
            "#,
        )
        .bind(code)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to find share: {}", e),
        })?
        .ok_or_else(|| DomainError::NotFound {
            resource: "share".to_string(),
        })?;

        parse_share_row(row)
    }

    async fn find_shares_by_entry(&self, entry_id: &EntryId) -> DomainResult<Vec<Share>> {
        let entry_id_str = entry_id.to_string();
        let rows: Vec<ShareRow> = sqlx::query_as(
            r#"
            SELECT
                id, namespace_id, entry_id, entry_version_id, code, expires_at,
                created_by, created_at, access_count, last_accessed_at, disabled_at
            FROM shares
            WHERE entry_id = ? AND disabled_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(entry_id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to find shares by entry: {}", e),
        })?;

        let mut shares = Vec::new();
        for row in rows {
            shares.push(parse_share_row(row)?);
        }

        Ok(shares)
    }

    async fn find_shares_by_user(&self, user_id: &UserId) -> DomainResult<Vec<Share>> {
        let user_id_str = user_id.to_string();
        let rows: Vec<ShareRow> = sqlx::query_as(
            r#"
            SELECT
                id, namespace_id, entry_id, entry_version_id, code, expires_at,
                created_by, created_at, access_count, last_accessed_at, disabled_at
            FROM shares
            WHERE created_by = ? AND disabled_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to find shares by user: {}", e),
        })?;

        let mut shares = Vec::new();
        for row in rows {
            shares.push(parse_share_row(row)?);
        }

        Ok(shares)
    }

    async fn find_shares_with_entry_by_user(
        &self,
        user_id: &UserId,
    ) -> DomainResult<Vec<ShareWithEntry>> {
        #[derive(sqlx::FromRow)]
        struct ShareWithEntryRow {
            id: String,
            namespace_id: String,
            entry_id: String,
            entry_version_id: Option<String>,
            code: String,
            expires_at: Option<String>,
            created_by: String,
            created_at: String,
            access_count: i64,
            last_accessed_at: Option<String>,
            disabled_at: Option<String>,
            entry_path: String,
            entry_kind: String,
        }

        let rows: Vec<ShareWithEntryRow> = sqlx::query_as(
            r#"
            SELECT
                s.id, s.namespace_id, s.entry_id, s.entry_version_id, s.code,
                s.expires_at, s.created_by, s.created_at, s.access_count,
                s.last_accessed_at, s.disabled_at,
                e.path AS entry_path, e.kind AS entry_kind
            FROM shares s
            JOIN entries e ON e.id = s.entry_id
            WHERE s.created_by = ? AND s.disabled_at IS NULL
            ORDER BY s.created_at DESC
            "#,
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to find shares with entry by user: {e}"),
        })?;

        let mut shares = Vec::with_capacity(rows.len());
        for row in rows {
            let kind = match row.entry_kind.as_str() {
                "directory" => EntryKind::Directory,
                _ => EntryKind::File,
            };
            shares.push(ShareWithEntry {
                // parse_share_row 接收元组（与其它查询保持一致）
                share: parse_share_row((
                    row.id,
                    row.namespace_id,
                    row.entry_id,
                    row.entry_version_id,
                    row.code,
                    row.expires_at,
                    row.created_by,
                    row.created_at,
                    row.access_count,
                    row.last_accessed_at,
                    row.disabled_at,
                ))?,
                entry_name: entry_name_from_path(&row.entry_path),
                entry_path: row.entry_path,
                entry_kind: kind,
            });
        }

        Ok(shares)
    }

    async fn find_share_by_code_including_expired(&self, code: &str) -> DomainResult<Share> {
        let row: ShareRow = sqlx::query_as(
            r#"
            SELECT
                id, namespace_id, entry_id, entry_version_id, code, expires_at,
                created_by, created_at, access_count, last_accessed_at, disabled_at
            FROM shares
            WHERE code = ? AND disabled_at IS NULL
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to find share: {}", e),
        })?
        .ok_or_else(|| DomainError::NotFound {
            resource: "share".to_string(),
        })?;

        parse_share_row(row)
    }

    async fn record_share_access(&self, share_id: &ShareId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();

        sqlx::query(
            r#"
            UPDATE shares
            SET access_count = access_count + 1, last_accessed_at = ?
            WHERE id = ? AND disabled_at IS NULL
            "#,
        )
        .bind(now)
        .bind(share_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to record share access: {}", e),
        })?;

        Ok(())
    }

    async fn disable_share(&self, share_id: &ShareId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();

        sqlx::query(
            r#"
            UPDATE shares
            SET disabled_at = ?
            WHERE id = ? AND disabled_at IS NULL
            "#,
        )
        .bind(now)
        .bind(share_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to disable share: {}", e),
        })?;

        Ok(())
    }

    async fn cleanup_expired_shares(&self) -> DomainResult<i64> {
        let now = time::OffsetDateTime::now_utc();

        let result = sqlx::query(
            r#"
            UPDATE shares
            SET disabled_at = ?
            WHERE expires_at IS NOT NULL AND expires_at <= ? AND disabled_at IS NULL
            "#,
        )
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to cleanup expired shares: {}", e),
        })?;

        Ok(result.rows_affected() as i64)
    }
}

#[derive(Debug)]
pub struct SqliteAdminRepo {
    pool: SqlitePool,
}

impl SqliteAdminRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl Clone for SqliteAdminRepo {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

#[async_trait::async_trait]
impl AdminRepo for SqliteAdminRepo {
    async fn list_users(&self, limit: i64, offset: i64) -> DomainResult<Vec<User>> {
        let rows: Vec<UserRow> = sqlx::query_as(
            "SELECT id, username, email, password_hash, role, disabled, created_at, updated_at, password_changed_at FROM users ORDER BY created_at DESC LIMIT ? OFFSET ?"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to list users: {}", e),
        })?;

        let mut users = Vec::new();
        for row in rows {
            users.push(user_from_row(row)?);
        }

        Ok(users)
    }

    async fn count_users(&self) -> DomainResult<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to count users: {}", e),
            })?;
        Ok(count)
    }

    async fn create_user(
        &self,
        username: &Username,
        email: &EmailAddress,
        password_hash: &str,
        role: Role,
    ) -> DomainResult<UserId> {
        let id = UserId::new();
        let now = time::OffsetDateTime::now_utc();
        let role_str = role_as_str(role);

        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role, disabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(username.as_str())
        .bind(email.as_str())
        .bind(password_hash)
        .bind(role_str)
        .bind(false)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal {
            message: format!("Failed to create user: {}", e),
        })?;

        Ok(id)
    }

    async fn update_user_role(&self, user_id: &UserId, role: Role) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        let role_str = role_as_str(role);

        sqlx::query("UPDATE users SET role = ?, updated_at = ? WHERE id = ?")
            .bind(role_str)
            .bind(now)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to update user role: {}", e),
            })?;
        Ok(())
    }

    async fn disable_user(&self, user_id: &UserId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query("UPDATE users SET disabled = ?, updated_at = ? WHERE id = ?")
            .bind(true)
            .bind(now)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to disable user: {}", e),
            })?;
        Ok(())
    }

    async fn enable_user(&self, user_id: &UserId) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query("UPDATE users SET disabled = ?, updated_at = ? WHERE id = ?")
            .bind(false)
            .bind(now)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to enable user: {}", e),
            })?;
        Ok(())
    }

    async fn delete_user(&self, user_id: &UserId) -> DomainResult<()> {
        // First check if user exists
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE id = ?")
            .bind(user_id.to_string())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to check user existence: {}", e),
            })?;

        if count == 0 {
            return Err(DomainError::NotFound {
                resource: "user".to_string(),
            });
        }

        // Delete user (cascade will handle related records)
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to delete user: {}", e),
            })?;
        Ok(())
    }

    async fn reset_user_password(
        &self,
        user_id: &UserId,
        new_password_hash: &str,
    ) -> DomainResult<()> {
        let now = time::OffsetDateTime::now_utc();
        sqlx::query("UPDATE users SET password_hash = ?, password_changed_at = ?, updated_at = ? WHERE id = ?")
            .bind(new_password_hash)
            .bind(now)
            .bind(now)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal {
                message: format!("Failed to reset user password: {}", e),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod blob_stream_tests {
    use super::*;
    use crate::{SqliteMigrations, SqlitePoolFactory};
    use camino::Utf8PathBuf;

    /// 回归：流中断时必须删除临时文件（FTP/批量导入中断后 tmp/ 不应堆积）。
    struct FailingReader {
        emitted: bool,
    }

    impl tokio::io::AsyncRead for FailingReader {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            if self.emitted {
                return std::task::Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    "client went away",
                )));
            }
            self.emitted = true;
            buf.put_slice(b"partial");
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn store_blob_stream_removes_temp_file_on_read_error() {
        // 与本文件其它用例一致：使用系统临时目录 + 随机后缀（crate 无 tempfile 依赖）
        let storage_root = Utf8PathBuf::from_path_buf(
            std::env::temp_dir().join(format!("vfiles-blob-stream-{}", uuid::Uuid::new_v4())),
        )
        .expect("temp path should be valid utf-8");
        tokio::fs::create_dir_all(&storage_root)
            .await
            .expect("temp dir should be created");

        let pool = SqlitePoolFactory::connect(storage_root.join("vfiles.db").as_path())
            .await
            .expect("pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should succeed");
        let store = FsBlobStore::new(pool, storage_root.join("blobs"));

        let result = store
            .store_blob_stream(Box::new(FailingReader { emitted: false }), None)
            .await;
        assert!(result.is_err(), "读取失败应返回错误，实际: {result:?}");

        let mut leftovers = Vec::new();
        // 临时文件位于 blob 根目录下的 tmp/（FsBlobStore::base_path.join("tmp")）
        let mut dir = tokio::fs::read_dir(storage_root.join("blobs").join("tmp"))
            .await
            .expect("tmp dir should exist");
        while let Some(entry) = dir.next_entry().await.expect("dir entry") {
            leftovers.push(entry.file_name().to_string_lossy().to_string());
        }

        let _ = tokio::fs::remove_dir_all(&storage_root).await;

        assert!(
            leftovers.is_empty(),
            "中断后不应残留临时文件，实际: {leftovers:?}"
        );
    }
}

#[cfg(test)]
mod snapshot_repo_tests {
    use super::*;
    use crate::{SqliteMigrations, SqlitePoolFactory};
    use camino::Utf8PathBuf;
    use std::sync::Arc;
    use tokio::sync::Barrier;

    async fn setup_snapshot_repo() -> (
        Utf8PathBuf,
        SqlitePool,
        SqliteSnapshotRepo,
        NamespaceId,
        UserId,
    ) {
        let db_path = Utf8PathBuf::from_path_buf(
            std::env::temp_dir().join(format!("vfiles-snapshot-test-{}.db", uuid::Uuid::new_v4())),
        )
        .expect("temp path should be valid utf-8");
        let pool = SqlitePoolFactory::connect(&db_path)
            .await
            .expect("sqlite pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let snapshot_repo = SqliteSnapshotRepo::new(pool.clone());

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed-password")
            .await
            .expect("admin user should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("default namespace should be created");

        (db_path, pool, snapshot_repo, namespace_id, user_id)
    }

    async fn cleanup_db(pool: SqlitePool, db_path: Utf8PathBuf) {
        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn create_snapshot_uses_next_available_suffix_after_gap() {
        let (db_path, pool, repo, namespace_id, user_id) = setup_snapshot_repo().await;

        let first = repo
            .create_snapshot(
                &namespace_id,
                Some("one"),
                SnapshotKind::AutoCommit,
                &user_id,
            )
            .await
            .expect("first snapshot should be created");
        let second = repo
            .create_snapshot(
                &namespace_id,
                Some("two"),
                SnapshotKind::AutoCommit,
                &user_id,
            )
            .await
            .expect("second snapshot should be created");
        let third = repo
            .create_snapshot(
                &namespace_id,
                Some("three"),
                SnapshotKind::AutoCommit,
                &user_id,
            )
            .await
            .expect("third snapshot should be created");

        let _ = first;

        sqlx::query("DELETE FROM snapshots WHERE id = ?")
            .bind(second.to_string())
            .execute(&pool)
            .await
            .expect("snapshot gap should be created");

        let fourth = repo
            .create_snapshot(
                &namespace_id,
                Some("four"),
                SnapshotKind::AutoCommit,
                &user_id,
            )
            .await
            .expect("snapshot creation should skip over existing suffixes");

        let third_snapshot = repo
            .find_snapshot(&third)
            .await
            .expect("third snapshot should still exist");
        let fourth_snapshot = repo
            .find_snapshot(&fourth)
            .await
            .expect("fourth snapshot should exist");

        assert_eq!(third_snapshot.snapshot_no, 3);
        assert_eq!(fourth_snapshot.snapshot_no, 4);

        cleanup_db(pool, db_path).await;
    }

    #[tokio::test]
    async fn create_snapshot_allows_parallel_creates() {
        let (db_path, pool, repo, namespace_id, user_id) = setup_snapshot_repo().await;
        let repo = Arc::new(repo);
        let task_count = 8;
        let barrier = Arc::new(Barrier::new(task_count + 1));
        let mut handles = Vec::new();

        for _ in 0..task_count {
            let repo = Arc::clone(&repo);
            let barrier = Arc::clone(&barrier);

            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                repo.create_snapshot(
                    &namespace_id,
                    Some("parallel create"),
                    SnapshotKind::AutoCommit,
                    &user_id,
                )
                .await
            }));
        }

        barrier.wait().await;

        let mut snapshot_nos = Vec::new();
        for handle in handles {
            let snapshot_id = handle
                .await
                .expect("snapshot task should join")
                .expect("parallel snapshot creation should succeed");
            let snapshot = repo
                .find_snapshot(&snapshot_id)
                .await
                .expect("created snapshot should be readable");
            snapshot_nos.push(snapshot.snapshot_no);
        }

        snapshot_nos.sort_unstable();
        assert_eq!(snapshot_nos, (1..=task_count as u32).collect::<Vec<_>>());

        cleanup_db(pool, db_path).await;
    }
}

#[cfg(test)]
mod audit_log_tests {
    use super::*;
    use crate::{SqliteMigrations, SqlitePoolFactory};
    use camino::Utf8PathBuf;

    async fn setup() -> (Utf8PathBuf, SqlitePool, SqliteAuditLogRepo) {
        let db_path = Utf8PathBuf::from_path_buf(
            std::env::temp_dir().join(format!("vfiles-audit-test-{}.db", uuid::Uuid::new_v4())),
        )
        .expect("temp path should be valid utf-8");
        let pool = SqlitePoolFactory::connect(&db_path)
            .await
            .expect("sqlite pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");
        let repo = SqliteAuditLogRepo::new(pool.clone());
        (db_path, pool, repo)
    }

    fn sample(action: &str, username: &str) -> NewAuditLog {
        NewAuditLog::success(action).user(None, username).request(
            Some("203.0.113.7".to_string()),
            Some("Mozilla/5.0".to_string()),
        )
    }

    /// 数据库触发器必须拒绝修改与删除（「只读、不可删除、不可修改」的硬保证）。
    #[tokio::test]
    async fn audit_logs_cannot_be_updated_or_deleted() {
        let (db_path, pool, repo) = setup().await;
        repo.append(&sample("login.success", "alice"))
            .await
            .expect("append should succeed");

        let update = sqlx::query("UPDATE audit_logs SET username = 'mallory'")
            .execute(&pool)
            .await;
        assert!(update.is_err(), "UPDATE 必须被触发器拒绝");

        let delete = sqlx::query("DELETE FROM audit_logs").execute(&pool).await;
        assert!(delete.is_err(), "DELETE 必须被触发器拒绝");

        let page = repo
            .list(&AuditLogQuery {
                limit: 50,
                ..Default::default()
            })
            .await
            .expect("list should succeed");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].username, "alice", "原记录不应被改动");

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn list_filters_by_keyword_action_result_and_time() {
        let (db_path, pool, repo) = setup().await;
        repo.append(&sample("login.success", "alice"))
            .await
            .unwrap();
        repo.append(&sample("file.upload", "bob")).await.unwrap();
        repo.append(
            &NewAuditLog::failure("login.failure")
                .user(None, "carol")
                .request(Some("198.51.100.9".to_string()), None),
        )
        .await
        .unwrap();

        // 关键字命中用户名或 IP
        let by_user = repo
            .list(&AuditLogQuery {
                keyword: Some("bob".to_string()),
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_user.total, 1);
        assert_eq!(by_user.items[0].action, "file.upload");

        let by_ip = repo
            .list(&AuditLogQuery {
                keyword: Some("198.51.100".to_string()),
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_ip.total, 1);
        assert_eq!(by_ip.items[0].username, "carol");

        // 动作与结果筛选
        let by_action = repo
            .list(&AuditLogQuery {
                action: Some("login.success".to_string()),
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_action.total, 1);

        let failures = repo
            .list(&AuditLogQuery {
                result: Some(AuditResult::Failure),
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(failures.total, 1);
        assert_eq!(failures.items[0].result, AuditResult::Failure);

        // 时间范围：since 设为未来则查不到
        let future = repo
            .list(&AuditLogQuery {
                since: Some(time::OffsetDateTime::now_utc() + time::Duration::hours(1)),
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(future.total, 0);

        // 分页：倒序返回，limit/offset 生效
        let first_page = repo
            .list(&AuditLogQuery {
                limit: 2,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(first_page.total, 3);
        assert_eq!(first_page.items.len(), 2);

        let second_page = repo
            .list(&AuditLogQuery {
                limit: 2,
                offset: 2,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(second_page.items.len(), 1);

        let actions = repo.distinct_actions().await.unwrap();
        assert_eq!(
            actions,
            vec![
                "file.upload".to_string(),
                "login.failure".to_string(),
                "login.success".to_string()
            ]
        );

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    /// 兼容 SQLite `datetime('now')` 格式的历史/手工数据（不能让整表读不出来）。
    #[tokio::test]
    async fn list_accepts_sqlite_datetime_format() {
        let (db_path, pool, repo) = setup().await;
        repo.append(&sample("login.success", "alice"))
            .await
            .unwrap();

        sqlx::query(
            r#"
            INSERT INTO audit_logs (id, created_at, user_id, username, action, result)
            VALUES ('legacy-1', '2026-01-02 03:04:05', NULL, 'legacy', 'login.success', 'success')
            "#,
        )
        .execute(&pool)
        .await
        .expect("insert legacy row");

        let page = repo
            .list(&AuditLogQuery {
                limit: 50,
                ..Default::default()
            })
            .await
            .expect("list should tolerate the sqlite datetime format");

        assert_eq!(page.items.len(), 2, "两条记录都应可读");
        let legacy = page
            .items
            .iter()
            .find(|item| item.username == "legacy")
            .expect("legacy row should be parsed");
        assert_eq!(legacy.created_at.year(), 2026);
        assert_eq!(legacy.created_at.hour(), 3);

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    /// 概览聚合：总量、失败数与 Top 用户/动作，并且尊重筛选。
    #[tokio::test]
    async fn summary_counts_totals_and_top_keys() {
        let (db_path, pool, repo) = setup().await;
        for _ in 0..3 {
            repo.append(&sample("login.success", "alice"))
                .await
                .unwrap();
        }
        repo.append(&sample("file.download", "alice"))
            .await
            .unwrap();
        repo.append(&sample("file.upload", "bob")).await.unwrap();
        // sample() 默认 success，失败记录要显式构造
        repo.append(
            &NewAuditLog::failure("login.failure")
                .user(None, "bob")
                .request(Some("198.51.100.4".to_string()), None),
        )
        .await
        .unwrap();
        // 匿名（username 为空）会被归到 (匿名)
        repo.append(&NewAuditLog::failure("login.failure").user(None, ""))
            .await
            .unwrap();

        let all = repo
            .summarize(&AuditLogQuery::default(), 5)
            .await
            .expect("summarize");
        assert_eq!(all.total, 7);
        assert_eq!(all.failures, 2);
        assert_eq!(
            all.users.first().map(|item| item.key.as_str()),
            Some("alice")
        );
        assert_eq!(all.users.first().map(|item| item.count), Some(4));
        assert_eq!(
            all.actions.first().map(|item| item.key.as_str()),
            Some("login.success")
        );
        assert_eq!(all.actions.first().map(|item| item.count), Some(3));
        assert!(
            all.users.iter().any(|item| item.key == "(匿名)"),
            "匿名记录应计入 (匿名): {:?}",
            all.users
        );

        // 只看失败：用户/动作统计随之变化
        let failures = repo
            .summarize(
                &AuditLogQuery {
                    result: Some(AuditResult::Failure),
                    ..Default::default()
                },
                5,
            )
            .await
            .unwrap();
        assert_eq!(failures.total, 2);
        assert_eq!(failures.failures, 2);
        assert_eq!(failures.actions.len(), 1);
        assert_eq!(failures.actions[0].key, "login.failure");
        assert_eq!(failures.users.len(), 2);

        // Top 限制生效
        let top_two = repo.summarize(&AuditLogQuery::default(), 2).await.unwrap();
        assert_eq!(top_two.users.len(), 2);

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn device_is_described_from_user_agent() {
        let (db_path, pool, repo) = setup().await;
        repo.append(
            &NewAuditLog::success("login.success")
                .user(None, "alice")
                .request(
                    Some("203.0.113.7".to_string()),
                    Some("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1".to_string()),
                ),
        )
        .await
        .unwrap();

        let page = repo
            .list(&AuditLogQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.items[0].device.as_deref(), Some("iPhone · Safari"));
        assert_eq!(page.items[0].ip.as_deref(), Some("203.0.113.7"));

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}

#[cfg(test)]
mod entry_version_batch_tests {
    use super::*;
    use crate::{SqliteMigrations, SqlitePoolFactory};
    use camino::Utf8PathBuf;

    async fn setup() -> (
        Utf8PathBuf,
        SqlitePool,
        SqliteEntryRepo,
        NamespaceId,
        UserId,
    ) {
        let db_path = Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!(
            "vfiles-entry-version-test-{}.db",
            uuid::Uuid::new_v4()
        )))
        .expect("temp path should be valid utf-8");
        let pool = SqlitePoolFactory::connect(&db_path)
            .await
            .expect("sqlite pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed-password")
            .await
            .expect("admin user should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("default namespace should be created");

        (db_path, pool, entry_repo, namespace_id, user_id)
    }

    #[tokio::test]
    async fn find_versions_fetches_many_and_skips_missing() {
        let (db_path, pool, repo, namespace_id, user_id) = setup().await;
        let path = NormalizedPath::new("docs/a.txt").expect("path should parse");
        let entry_id = repo
            .create_entry(&namespace_id, &path, EntryKind::File, &user_id)
            .await
            .expect("entry should be created");

        let first = repo
            .create_version(
                &entry_id,
                None,
                None,
                10,
                Some("text/plain"),
                &user_id,
                Some("one"),
            )
            .await
            .expect("first version should be created");
        let second = repo
            .create_version(
                &entry_id,
                None,
                None,
                20,
                Some("text/plain"),
                &user_id,
                Some("two"),
            )
            .await
            .expect("second version should be created");

        let versions = repo
            .find_versions(&[first.id, second.id])
            .await
            .expect("batch lookup should succeed");
        assert_eq!(versions.len(), 2);

        assert!(
            repo.find_versions(&[])
                .await
                .expect("empty lookup should succeed")
                .is_empty()
        );

        let only_first = repo
            .find_versions(&[first.id, VersionId::new()])
            .await
            .expect("lookup with missing id should succeed");
        assert_eq!(only_first.len(), 1);
        assert_eq!(only_first[0].id, first.id);

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}

#[cfg(test)]
mod entry_repo_lookup_tests {
    use super::*;
    use crate::{SqliteMigrations, SqlitePoolFactory};
    use camino::Utf8PathBuf;

    async fn setup() -> (
        Utf8PathBuf,
        SqlitePool,
        SqliteEntryRepo,
        NamespaceId,
        UserId,
    ) {
        let db_path = Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!(
            "vfiles-entry-lookup-test-{}.db",
            uuid::Uuid::new_v4()
        )))
        .expect("temp path should be valid utf-8");
        let pool = SqlitePoolFactory::connect(&db_path)
            .await
            .expect("sqlite pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed-password")
            .await
            .expect("admin user should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("default namespace should be created");

        (db_path, pool, entry_repo, namespace_id, user_id)
    }

    #[tokio::test]
    async fn find_all_returns_every_entry_sorted_with_current_versions() {
        let (db_path, pool, repo, namespace_id, user_id) = setup().await;

        for (path, kind) in [
            ("docs", EntryKind::Directory),
            ("docs/a.txt", EntryKind::File),
            ("b.txt", EntryKind::File),
        ] {
            repo.create_entry(
                &namespace_id,
                &NormalizedPath::new(path).expect("path should parse"),
                kind,
                &user_id,
            )
            .await
            .expect("entry should be created");
        }

        let docs = repo
            .find_by_path(
                &namespace_id,
                &NormalizedPath::new("docs/a.txt").expect("path should parse"),
            )
            .await
            .expect("lookup should succeed")
            .expect("entry should exist");
        repo.create_version(
            &docs.id,
            None,
            None,
            42,
            Some("text/plain"),
            &user_id,
            Some("initial"),
        )
        .await
        .expect("version should be created");

        let all = repo
            .find_all(&namespace_id)
            .await
            .expect("find_all should succeed");

        let paths: Vec<&str> = all.iter().map(|entry| entry.path_norm.as_str()).collect();
        assert_eq!(paths, vec!["b.txt", "docs", "docs/a.txt"]);

        let file = all
            .iter()
            .find(|entry| entry.path_norm.as_str() == "docs/a.txt")
            .expect("file should be present");
        assert!(file.current_version_id.is_some());

        let directory = all
            .iter()
            .find(|entry| entry.path_norm.as_str() == "docs")
            .expect("directory should be present");
        assert!(directory.current_version_id.is_none());

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}

#[cfg(test)]
mod entry_repo_subtree_tests {
    use super::*;
    use crate::{SqliteMigrations, SqlitePoolFactory};
    use camino::Utf8PathBuf;

    async fn setup() -> (
        Utf8PathBuf,
        SqlitePool,
        SqliteEntryRepo,
        NamespaceId,
        UserId,
    ) {
        let db_path = Utf8PathBuf::from_path_buf(
            std::env::temp_dir().join(format!("vfiles-subtree-test-{}.db", uuid::Uuid::new_v4())),
        )
        .expect("temp path should be valid utf-8");
        let pool = SqlitePoolFactory::connect(&db_path)
            .await
            .expect("sqlite pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed-password")
            .await
            .expect("admin user should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("default namespace should be created");

        (db_path, pool, entry_repo, namespace_id, user_id)
    }

    #[tokio::test]
    async fn find_subtree_returns_only_the_requested_branch() {
        let (db_path, pool, repo, namespace_id, user_id) = setup().await;

        for (path, kind) in [
            ("docs", EntryKind::Directory),
            ("docs/a.txt", EntryKind::File),
            ("docs/nested", EntryKind::Directory),
            ("docs/nested/b.txt", EntryKind::File),
            ("docs2", EntryKind::File),
            ("docs2/c.txt", EntryKind::File),
            ("other.txt", EntryKind::File),
        ] {
            repo.create_entry(
                &namespace_id,
                &NormalizedPath::new(path).expect("path should parse"),
                kind,
                &user_id,
            )
            .await
            .expect("entry should be created");
        }

        let docs = repo
            .find_subtree(
                &namespace_id,
                &NormalizedPath::new("docs").expect("path should parse"),
            )
            .await
            .expect("subtree lookup should succeed");
        let paths: Vec<&str> = docs.iter().map(|entry| entry.path_norm.as_str()).collect();
        assert_eq!(
            paths,
            vec!["docs", "docs/a.txt", "docs/nested", "docs/nested/b.txt"]
        );

        let docs2 = repo
            .find_subtree(
                &namespace_id,
                &NormalizedPath::new("docs2").expect("path should parse"),
            )
            .await
            .expect("subtree lookup should succeed");
        let paths: Vec<&str> = docs2.iter().map(|entry| entry.path_norm.as_str()).collect();
        assert_eq!(paths, vec!["docs2", "docs2/c.txt"]);

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}

#[cfg(test)]
mod entry_batch_cleanup_tests {
    use super::*;
    use crate::{SqliteMigrations, SqlitePoolFactory};
    use camino::Utf8PathBuf;

    async fn setup() -> (
        Utf8PathBuf,
        SqlitePool,
        SqliteEntryRepo,
        NamespaceId,
        UserId,
    ) {
        let db_path = Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!(
            "vfiles-batch-cleanup-test-{}.db",
            uuid::Uuid::new_v4()
        )))
        .expect("temp path should be valid utf-8");
        let pool = SqlitePoolFactory::connect(&db_path)
            .await
            .expect("sqlite pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed-password")
            .await
            .expect("admin user should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("default namespace should be created");

        (db_path, pool, entry_repo, namespace_id, user_id)
    }

    #[tokio::test]
    async fn batch_history_and_delete_cover_all_requested_entries() {
        let (db_path, pool, repo, namespace_id, user_id) = setup().await;

        let mut entry_ids = Vec::new();
        for path in ["docs/a.txt", "docs/b.txt"] {
            let entry_id = repo
                .create_entry(
                    &namespace_id,
                    &NormalizedPath::new(path).expect("path should parse"),
                    EntryKind::File,
                    &user_id,
                )
                .await
                .expect("entry should be created");
            for message in ["one", "two"] {
                repo.create_version(&entry_id, None, None, 5, None, &user_id, Some(message))
                    .await
                    .expect("version should be created");
            }
            entry_ids.push(entry_id);
        }

        let versions = repo
            .find_versions_for_entries(&entry_ids)
            .await
            .expect("batch history should succeed");
        assert_eq!(versions.len(), 4);
        assert!(
            repo.find_versions_for_entries(&[])
                .await
                .expect("empty batch should succeed")
                .is_empty()
        );

        repo.delete_entries(&entry_ids)
            .await
            .expect("delete should succeed");
        assert!(
            repo.find_all(&namespace_id)
                .await
                .expect("find_all should succeed")
                .is_empty()
        );

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}

#[cfg(test)]
mod entry_move_batch_tests {
    use super::*;
    use crate::{SqliteMigrations, SqlitePoolFactory};
    use camino::Utf8PathBuf;

    async fn setup() -> (
        Utf8PathBuf,
        SqlitePool,
        SqliteEntryRepo,
        NamespaceId,
        UserId,
    ) {
        let db_path = Utf8PathBuf::from_path_buf(std::env::temp_dir().join(format!(
            "vfiles-move-batch-test-{}.db",
            uuid::Uuid::new_v4()
        )))
        .expect("temp path should be valid utf-8");
        let pool = SqlitePoolFactory::connect(&db_path)
            .await
            .expect("sqlite pool should connect");
        SqliteMigrations::run(&pool)
            .await
            .expect("migrations should run");

        let user_repo = SqliteUserRepo::new(pool.clone());
        let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
        let entry_repo = SqliteEntryRepo::new(pool.clone());

        let user_id = user_repo
            .create_admin("admin", "admin@example.com", "hashed-password")
            .await
            .expect("admin user should be created");
        let namespace_id = namespace_repo
            .create_default(&user_id, "default")
            .await
            .expect("default namespace should be created");

        (db_path, pool, entry_repo, namespace_id, user_id)
    }

    async fn create(
        repo: &SqliteEntryRepo,
        ns: &NamespaceId,
        user: &UserId,
        path: &str,
    ) -> EntryId {
        repo.create_entry(
            ns,
            &NormalizedPath::new(path).expect("path should parse"),
            EntryKind::File,
            user,
        )
        .await
        .expect("entry should be created")
    }

    #[tokio::test]
    async fn find_paths_and_move_entries_work_in_batch() {
        let (db_path, pool, repo, namespace_id, user_id) = setup().await;
        let a = create(&repo, &namespace_id, &user_id, "docs/a.txt").await;
        let b = create(&repo, &namespace_id, &user_id, "docs/b.txt").await;

        let found = repo
            .find_paths(
                &namespace_id,
                &[
                    NormalizedPath::new("docs/a.txt").expect("path should parse"),
                    NormalizedPath::new("missing.txt").expect("path should parse"),
                    NormalizedPath::new("docs/b.txt").expect("path should parse"),
                ],
            )
            .await
            .expect("find_paths should succeed");
        let mut paths: Vec<&str> = found.iter().map(|entry| entry.path_norm.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["docs/a.txt", "docs/b.txt"]);
        assert!(
            repo.find_paths(&namespace_id, &[])
                .await
                .expect("empty find_paths should succeed")
                .is_empty()
        );

        repo.move_entries(&[
            (
                a,
                NormalizedPath::new("docs/moved-a.txt").expect("path should parse"),
            ),
            (
                b,
                NormalizedPath::new("docs/moved-b.txt").expect("path should parse"),
            ),
        ])
        .await
        .expect("batch move should succeed");

        assert!(
            repo.find_by_path(
                &namespace_id,
                &NormalizedPath::new("docs/moved-a.txt").expect("path should parse"),
            )
            .await
            .expect("lookup should succeed")
            .is_some()
        );
        assert!(
            repo.find_by_path(
                &namespace_id,
                &NormalizedPath::new("docs/moved-b.txt").expect("path should parse"),
            )
            .await
            .expect("lookup should succeed")
            .is_some()
        );

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn move_entries_reports_path_conflicts() {
        let (db_path, pool, repo, namespace_id, user_id) = setup().await;
        create(&repo, &namespace_id, &user_id, "docs/a.txt").await;
        let b = create(&repo, &namespace_id, &user_id, "docs/b.txt").await;

        let result = repo
            .move_entries(&[(
                b,
                NormalizedPath::new("docs/a.txt").expect("path should parse"),
            )])
            .await;

        assert!(matches!(result, Err(DomainError::PathConflict { .. })));

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}
