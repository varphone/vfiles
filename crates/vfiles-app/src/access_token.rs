//! 访问令牌：给 CLI / 构建系统等程序使用的 API 凭证。
//!
//! - 明文形如 `vfat_<64 位十六进制>`，**只在创建时返回一次**；
//! - 库里只存 SHA-256 摘要（高熵随机串，无需慢哈希）与展示用前缀；
//! - 鉴权成功后刷新 `last_used_at`（尽力而为，失败不影响请求）；
//! - 支持可选过期时间与撤销。

use std::sync::Arc;

use sha2::{Digest, Sha256};

use vfiles_domain::{
    AccessToken, AccessTokenId, AccessTokenRepo, DomainError, DomainResult, NewAccessToken, UserId,
};

/// 明文前缀：便于在日志/界面上识别这是 VFiles 访问令牌。
const TOKEN_PREFIX: &str = "vfat_";

/// 允许的有效期（天）：0 表示永久。
pub const ALLOWED_EXPIRY_DAYS: [u32; 4] = [0, 30, 90, 365];

#[derive(Clone)]
pub struct AccessTokenService {
    repo: Arc<dyn AccessTokenRepo + Send + Sync>,
}

/// 创建结果：明文只在这里出现一次。
#[derive(Debug, Clone)]
pub struct CreatedAccessToken {
    pub token: AccessToken,
    pub plaintext: String,
}

impl AccessTokenService {
    pub fn new(repo: Arc<dyn AccessTokenRepo + Send + Sync>) -> Self {
        Self { repo }
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

    /// 列出某用户的令牌。
    pub async fn list(&self, user_id: &UserId) -> DomainResult<Vec<AccessToken>> {
        self.repo.list_for_user(user_id).await
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

        // 尽力而为：记录最近使用时间
        let _ = self.repo.touch_last_used(&token.id, now).await;

        Ok(Some(token))
    }
}

/// SHA-256 十六进制摘要。
pub fn hash_token(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}
