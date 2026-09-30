//! 访问令牌：给 CLI / 构建系统等程序使用的 API 凭证。
//!
//! - 明文形如 `vfat_<64 位十六进制>`，**只在创建时返回一次**；
//! - 库里只存 SHA-256 摘要（高熵随机串，无需慢哈希）与展示用前缀；
//! - 鉴权成功后刷新 `last_used_at`（尽力而为，失败不影响请求）；
//! - 支持可选过期时间与撤销。

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

use vfiles_domain::{
    AccessToken, AccessTokenId, AccessTokenPage, AccessTokenRepo, DomainError, DomainResult,
    NewAccessToken, UserId,
};

/// 明文前缀：便于在日志/界面上识别这是 VFiles 访问令牌。
const TOKEN_PREFIX: &str = "vfat_";
const LAST_USED_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const MAX_TRACKED_TOKEN_REFRESHES: usize = 50_000;

/// 允许的有效期（天）：0 表示永久。
pub const ALLOWED_EXPIRY_DAYS: [u32; 4] = [0, 30, 90, 365];

#[derive(Clone)]
pub struct AccessTokenService {
    repo: Arc<dyn AccessTokenRepo + Send + Sync>,
    last_used_refreshes: Arc<LastUsedRefreshCache>,
}

#[derive(Default)]
struct LastUsedRefreshCache {
    refreshed_at: Mutex<HashMap<AccessTokenId, Instant>>,
}

impl LastUsedRefreshCache {
    fn reserve(&self, token_id: AccessTokenId) -> Option<Instant> {
        self.reserve_at(token_id, Instant::now())
    }

    fn reserve_at(&self, token_id: AccessTokenId, now: Instant) -> Option<Instant> {
        let mut refreshed_at = self
            .refreshed_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(last_refresh) = refreshed_at.get_mut(&token_id) {
            if now.saturating_duration_since(*last_refresh) < LAST_USED_REFRESH_INTERVAL {
                return None;
            }
            *last_refresh = now;
            return Some(now);
        }

        if refreshed_at.len() >= MAX_TRACKED_TOKEN_REFRESHES
            && let Some(victim) = refreshed_at.keys().next().copied()
        {
            refreshed_at.remove(&victim);
        }
        refreshed_at.insert(token_id, now);
        Some(now)
    }

    fn clear_failed(&self, token_id: AccessTokenId, attempted_at: Instant) {
        let mut refreshed_at = self
            .refreshed_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if refreshed_at.get(&token_id) == Some(&attempted_at) {
            refreshed_at.remove(&token_id);
        }
    }
}

/// 创建结果：明文只在这里出现一次。
#[derive(Debug, Clone)]
pub struct CreatedAccessToken {
    pub token: AccessToken,
    pub plaintext: String,
}

impl AccessTokenService {
    pub fn new(repo: Arc<dyn AccessTokenRepo + Send + Sync>) -> Self {
        Self {
            repo,
            last_used_refreshes: Arc::new(LastUsedRefreshCache::default()),
        }
    }

    /// 生成新令牌（明文返回给调用方，库里只留摘要）。
    pub async fn create(
        &self,
        user_id: &UserId,
        name: &str,
        expires_in_days: Option<u32>,
    ) -> DomainResult<CreatedAccessToken> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::Validation {
                message: "Token name is required".to_string(),
            });
        }
        if name.chars().count() > 64 {
            return Err(DomainError::Validation {
                message: "Token name is too long (max 64 characters)".to_string(),
            });
        }

        let days = expires_in_days.unwrap_or(0);
        if !ALLOWED_EXPIRY_DAYS.contains(&days) {
            return Err(DomainError::Validation {
                message: format!(
                    "Unsupported expiry: {days} days (allowed: {:?})",
                    ALLOWED_EXPIRY_DAYS
                ),
            });
        }

        // 256 位随机：两个 UUIDv4 去掉连字符拼接
        let plaintext = format!(
            "{TOKEN_PREFIX}{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let token_hash = hash_token(&plaintext);
        let token_prefix: String = plaintext.chars().take(TOKEN_PREFIX.len() + 8).collect();
        let expires_at = (days > 0)
            .then(|| time::OffsetDateTime::now_utc() + time::Duration::days(i64::from(days)));

        let token = self
            .repo
            .create(&NewAccessToken {
                id: AccessTokenId::new(),
                user_id: *user_id,
                name: name.to_string(),
                token_hash,
                token_prefix,
                scopes: "full".to_string(),
                expires_at,
            })
            .await?;

        Ok(CreatedAccessToken { token, plaintext })
    }

    /// 分页列出某用户的令牌。
    pub async fn list(
        &self,
        user_id: &UserId,
        limit: u32,
        offset: u32,
    ) -> DomainResult<AccessTokenPage> {
        self.repo.list_for_user(user_id, limit, offset).await
    }

    /// 撤销令牌（只能撤销自己的）。
    pub async fn revoke(&self, user_id: &UserId, id: &AccessTokenId) -> DomainResult<()> {
        if self.repo.revoke(user_id, id).await? {
            Ok(())
        } else {
            Err(DomainError::NotFound {
                resource: format!("access token {id}"),
            })
        }
    }

    /// 用明文令牌换出所属用户；无效/过期/已撤销返回 `None`。
    pub async fn authenticate(&self, plaintext: &str) -> DomainResult<Option<AccessToken>> {
        let plaintext = plaintext.trim();
        if !plaintext.starts_with(TOKEN_PREFIX) {
            return Ok(None);
        }

        let token_hash = hash_token(plaintext);
        let Some(token) = self.repo.find_by_hash(&token_hash).await? else {
            return Ok(None);
        };

        let now = time::OffsetDateTime::now_utc();
        if !token.is_active(now) {
            return Ok(None);
        }

        // 高频 API 请求不必每次都把 SQLite 转成写事务；失败时立即允许后续重试。
        if let Some(attempted_at) = self.last_used_refreshes.reserve(token.id)
            && self.repo.touch_last_used(&token.id, now).await.is_err()
        {
            self.last_used_refreshes
                .clear_failed(token.id, attempted_at);
        }

        Ok(Some(token))
    }
}

/// SHA-256 十六进制摘要。
pub fn hash_token(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{AccessTokenId, LAST_USED_REFRESH_INTERVAL, LastUsedRefreshCache};
    use std::time::{Duration, Instant};

    #[test]
    fn refresh_cache_throttles_touches_and_reopens_after_one_minute() {
        let cache = LastUsedRefreshCache::default();
        let token_id = AccessTokenId::new();
        let first = Instant::now();

        assert_eq!(cache.reserve_at(token_id, first), Some(first));
        assert_eq!(
            cache.reserve_at(token_id, first + Duration::from_secs(30)),
            None
        );
        let after_interval = first + LAST_USED_REFRESH_INTERVAL + Duration::from_secs(1);
        assert_eq!(
            cache.reserve_at(token_id, after_interval),
            Some(after_interval)
        );
    }

    #[test]
    fn failed_refresh_can_be_retried_immediately() {
        let cache = LastUsedRefreshCache::default();
        let token_id = AccessTokenId::new();
        let attempted_at = Instant::now();

        assert_eq!(cache.reserve_at(token_id, attempted_at), Some(attempted_at));
        cache.clear_failed(token_id, attempted_at);
        assert_eq!(
            cache.reserve_at(token_id, attempted_at + Duration::from_secs(1)),
            Some(attempted_at + Duration::from_secs(1))
        );
    }
}
