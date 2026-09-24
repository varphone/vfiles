//! WebDAV exclusive write locks backed by the configured lock repository.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vfiles_domain::{NamespaceId, WebdavLockRepo};

const MAX_TIMEOUT_SECONDS: u64 = u32::MAX as u64;

/// Active lock data used by protocol response generation.
#[derive(Debug, Clone)]
pub struct LockEntry {
    pub token: String,
    pub owner: String,
    pub path: String,
    pub depth_infinity: bool,
    pub scope: vfiles_domain::WebdavLockScope,
    /// Unix milliseconds; `None` represents an infinite lock.
    pub expires_at: Option<i64>,
}

pub struct LockTable {
    repo: Arc<dyn WebdavLockRepo>,
}

impl LockTable {
    pub fn new(repo: Arc<dyn WebdavLockRepo>) -> Self {
        Self { repo }
    }

    pub(crate) fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or(0)
    }

    fn expires_at(ttl: Option<Duration>, now: i64) -> Option<i64> {
        ttl.map(|duration| now.saturating_add(duration.as_millis().min(i64::MAX as u128) as i64))
    }

    fn from_record(path: &str, lock: vfiles_domain::WebdavLock) -> LockEntry {
        LockEntry {
            token: lock.token,
            owner: lock.owner,
            path: path.to_string(),
            expires_at: lock.expires_at,
            depth_infinity: lock.depth_infinity,
            scope: lock.scope,
        }
    }

    pub async fn lock(
        &self,
        namespace_id: &NamespaceId,
        path: &str,
        owner: &str,
        depth_infinity: bool,
        ttl: Option<Duration>,
        scope: vfiles_domain::WebdavLockScope,
    ) -> vfiles_domain::DomainResult<Option<LockEntry>> {
        let now = Self::now();
        let token = format!("opaquelocktoken:{}", uuid::Uuid::new_v4());
        let acquired = self
            .repo
            .acquire(
                namespace_id,
                path,
                vfiles_domain::NewWebdavLock {
                    token: &token,
                    owner,
                    depth_infinity,
                    scope,
                    expires_at: Self::expires_at(ttl, now),
                    now,
                },
            )
            .await?;
        Ok(acquired.then(|| LockEntry {
            token,
            owner: owner.to_string(),
            path: path.to_string(),
            expires_at: Self::expires_at(ttl, now),
            depth_infinity,
            scope,
        }))
    }

    pub async fn unlock(
        &self,
        namespace_id: &NamespaceId,
        path: &str,
        token: &str,
    ) -> vfiles_domain::DomainResult<bool> {
        self.repo
            .release(namespace_id, path, token, Self::now())
            .await
    }

    pub async fn remove_under_path(
        &self,
        namespace_id: &NamespaceId,
        path: &str,
    ) -> vfiles_domain::DomainResult<()> {
        self.repo.remove_under_path(namespace_id, path).await
    }

    pub async fn refresh(
        &self,
        namespace_id: &NamespaceId,
        path: &str,
        token: &str,
        ttl: Option<Duration>,
    ) -> vfiles_domain::DomainResult<Option<LockEntry>> {
        let now = Self::now();
        Ok(self
            .repo
            .refresh(namespace_id, path, token, Self::expires_at(ttl, now), now)
            .await?
            .map(|lock| Self::from_record(path, lock)))
    }

    pub async fn blocked(
        &self,
        namespace_id: &NamespaceId,
        path: &str,
    ) -> vfiles_domain::DomainResult<Option<LockEntry>> {
        Ok(self
            .repo
            .find_active_covering(namespace_id, path, Self::now())
            .await?
            .map(|(lock_path, lock)| Self::from_record(&lock_path, lock)))
    }

    pub async fn blocked_all(
        &self,
        namespace_id: &NamespaceId,
        path: &str,
    ) -> vfiles_domain::DomainResult<Vec<LockEntry>> {
        Ok(self
            .repo
            .find_active_covering_all(namespace_id, path, Self::now())
            .await?
            .into_iter()
            .map(|(lock_path, lock)| Self::from_record(&lock_path, lock))
            .collect())
    }

    pub async fn blocked_many(
        &self,
        namespace_id: &NamespaceId,
        paths: &[String],
    ) -> vfiles_domain::DomainResult<std::collections::HashMap<String, LockEntry>> {
        Ok(self
            .repo
            .find_active_covering_many(namespace_id, paths, Self::now())
            .await?
            .into_iter()
            .map(|(resource_path, (lock_path, lock))| {
                (resource_path, Self::from_record(&lock_path, lock))
            })
            .collect())
    }

    pub async fn blocked_many_all(
        &self,
        namespace_id: &NamespaceId,
        paths: &[String],
    ) -> vfiles_domain::DomainResult<std::collections::HashMap<String, Vec<LockEntry>>> {
        Ok(self
            .repo
            .find_active_covering_many_all(namespace_id, paths, Self::now())
            .await?
            .into_iter()
            .map(|(resource_path, locks)| {
                (
                    resource_path,
                    locks
                        .into_iter()
                        .map(|(lock_path, lock)| Self::from_record(&lock_path, lock))
                        .collect(),
                )
            })
            .collect())
    }

    pub async fn blocked_under_path(
        &self,
        namespace_id: &NamespaceId,
        path: &str,
    ) -> vfiles_domain::DomainResult<std::collections::HashMap<String, LockEntry>> {
        Ok(self
            .repo
            .find_active_under_path(namespace_id, path, Self::now())
            .await?
            .into_iter()
            .map(|(path, lock)| (path.clone(), Self::from_record(&path, lock)))
            .collect())
    }

    pub async fn blocked_under_path_all(
        &self,
        namespace_id: &NamespaceId,
        path: &str,
    ) -> vfiles_domain::DomainResult<Vec<LockEntry>> {
        Ok(self
            .repo
            .find_active_under_path_all(namespace_id, path, Self::now())
            .await?
            .into_iter()
            .map(|(lock_path, lock)| Self::from_record(&lock_path, lock))
            .collect())
    }

    /// Timeout header parser: the first valid `Second-N` alternative wins.
    /// RFC 4918 caps the value at 2^32-1 seconds.
    pub fn parse_timeout_header(value: &str) -> Option<Duration> {
        for part in value.split(',') {
            let value = part.trim();
            if value
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Second-"))
            {
                let digits = &value[7..];
                if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    let seconds = digits.parse::<u64>().unwrap_or(u64::MAX);
                    return Some(Duration::from_secs(seconds.min(MAX_TIMEOUT_SECONDS)));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_second_n_and_infinite_forms() {
        assert_eq!(
            LockTable::parse_timeout_header("Second-3600"),
            Some(Duration::from_secs(3600))
        );
        assert_eq!(
            LockTable::parse_timeout_header("Second-1, Infinite"),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            LockTable::parse_timeout_header("sEcOnD-17"),
            Some(Duration::from_secs(17))
        );
        assert_eq!(
            LockTable::parse_timeout_header("Second-4294967296"),
            Some(Duration::from_secs(u32::MAX as u64))
        );
        assert_eq!(
            LockTable::parse_timeout_header("Second-184467440737095516160"),
            Some(Duration::from_secs(u32::MAX as u64))
        );
        assert_eq!(
            LockTable::parse_timeout_header("Second- 17"),
            None,
            "whitespace inside a TimeType is invalid"
        );
        assert_eq!(LockTable::parse_timeout_header("Infinite"), None);
        assert_eq!(LockTable::parse_timeout_header("garbage"), None);
    }

    #[test]
    fn expiration_preserves_subsecond_precision() {
        assert_eq!(
            LockTable::expires_at(Some(Duration::from_millis(250)), 1_000),
            Some(1_250)
        );
        assert_eq!(LockTable::expires_at(None, 1_000), None);
    }
}
