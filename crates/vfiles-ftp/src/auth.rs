//! FTP 认证与用户信息。
//!
//! 复用 Web 端的凭据校验（`AuthService::verify_credentials`）与命名空间解析，
//! 因此 FTP 与 HTTP 的用户、角色、禁用状态、口令哈希升级规则完全一致。

use std::{fmt, path::Path, sync::Arc};

use async_trait::async_trait;
use tracing::warn;
use unftp_core::auth::{
    AuthenticationError, Authenticator, Credentials, Principal, UserDetail, UserDetailError,
    UserDetailProvider,
};
use vfiles_app::{
    AuthService, IngestStats, LoginAttemptLimiter, NamespaceService, RateLimitPolicy,
};
use vfiles_domain::{NamespaceId, Role, UserId, UserRepo, Username};

/// 登录后使用的用户信息：包含命名空间，决定该会话能看到哪些文件。
#[derive(Debug, Clone)]
pub struct VfilesFtpUser {
    pub id: UserId,
    pub username: String,
    pub role: Role,
    pub namespace_id: NamespaceId,
}

impl fmt::Display for VfilesFtpUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.username)
    }
}

impl UserDetail for VfilesFtpUser {
    fn account_enabled(&self) -> bool {
        true
    }

    /// `None` 表示以命名空间根为家目录：路径本身就带沙箱语义。
    fn home(&self) -> Option<&Path> {
        None
    }
}

/// 角色白名单：默认只允许管理员/管理角色使用 FTP 批量导入。
#[derive(Debug, Clone)]
pub struct RoleFilter {
    allowed: Vec<Role>,
}

impl RoleFilter {
    pub fn new(allowed: Vec<Role>) -> Self {
        Self { allowed }
    }

    /// 允许全部角色（用于显式放开限制的部署）。
    pub fn all() -> Self {
        Self {
            allowed: Vec::new(),
        }
    }

    pub fn is_allowed(&self, role: Role) -> bool {
        self.allowed.is_empty() || self.allowed.contains(&role)
    }
}

/// 认证器：校验口令、限制角色、按来源限流。
#[derive(Debug)]
pub struct VfilesAuthenticator {
    auth_service: Arc<AuthService>,
    roles: RoleFilter,
    limiter: Arc<LoginAttemptLimiter>,
    policy: RateLimitPolicy,
    stats: Arc<IngestStats>,
    /// 未启用认证时的兜底用户（仅在显式允许匿名时存在）。
    anonymous_username: Option<String>,
}

impl VfilesAuthenticator {
    pub fn new(
        auth_service: Arc<AuthService>,
        roles: RoleFilter,
        limiter: Arc<LoginAttemptLimiter>,
        policy: RateLimitPolicy,
        stats: Arc<IngestStats>,
    ) -> Self {
        Self {
            auth_service,
            roles,
            limiter,
            policy,
            stats,
            anonymous_username: None,
        }
    }

    /// 认证关闭时使用固定用户登录（调用方必须已确认允许匿名访问）。
    pub fn with_anonymous(mut self, username: impl Into<String>) -> Self {
        self.anonymous_username = Some(username.into());
        self
    }
}

#[async_trait]
impl Authenticator for VfilesAuthenticator {
    async fn authenticate(
        &self,
        username: &str,
        creds: &Credentials,
    ) -> Result<Principal, AuthenticationError> {
        if let Some(anonymous) = &self.anonymous_username {
            if username == anonymous {
                return Ok(Principal {
                    username: username.to_string(),
                });
            }
            return Err(AuthenticationError::BadUser);
        }

        let password = creds.password.clone().unwrap_or_default();
        let key = format!("{}|{username}", creds.source_ip);

        if self.limiter.check(&self.policy, &key).is_some() {
            self.stats.record_login_failure();
            warn!(username, "FTP 登录被限流拒绝");
            return Err(AuthenticationError::BadPassword);
        }

        match self
            .auth_service
            .verify_credentials(username, &password)
            .await
        {
            Ok(user) => {
                if user.disabled {
                    self.stats.record_login_failure();
                    return Err(AuthenticationError::BadUser);
                }
                if !self.roles.is_allowed(user.role) {
                    self.stats.record_login_failure();
                    warn!(username, role = ?user.role, "FTP 登录角色不被允许");
                    return Err(AuthenticationError::BadUser);
                }
                self.limiter.clear(&key);
                self.stats.record_login_success();
                Ok(Principal {
                    username: user.username.as_str().to_string(),
                })
            }
            Err(err) => {
                self.limiter.record_failure(&self.policy, &key);
                self.stats.record_login_failure();
                warn!(username, error = %err, "FTP 登录失败");
                Err(AuthenticationError::BadPassword)
            }
        }
    }
}

/// 把 `Principal` 补全为用户信息（角色 + 命名空间）。
pub struct VfilesUserDetailProvider {
    user_repo: Arc<dyn UserRepo + Send + Sync>,
    namespaces: NamespaceService,
    roles: RoleFilter,
    /// 匿名模式下的固定用户与命名空间。
    anonymous: Option<(UserId, NamespaceId)>,
}

impl fmt::Debug for VfilesUserDetailProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VfilesUserDetailProvider")
            .field("roles", &self.roles)
            .finish_non_exhaustive()
    }
}

impl VfilesUserDetailProvider {
    pub fn new(
        user_repo: Arc<dyn UserRepo + Send + Sync>,
        namespaces: NamespaceService,
        roles: RoleFilter,
    ) -> Self {
        Self {
            user_repo,
            namespaces,
            roles,
            anonymous: None,
        }
    }

    pub fn with_anonymous_user(mut self, user_id: UserId, namespace_id: NamespaceId) -> Self {
        self.anonymous = Some((user_id, namespace_id));
        self
    }
}

#[async_trait]
impl UserDetailProvider for VfilesUserDetailProvider {
    type User = VfilesFtpUser;

    async fn provide_user_detail(
        &self,
        principal: &Principal,
    ) -> Result<VfilesFtpUser, UserDetailError> {
        if let Some((user_id, namespace_id)) = self.anonymous {
            return Ok(VfilesFtpUser {
                id: user_id,
                username: principal.username.clone(),
                role: Role::Admin,
                namespace_id,
            });
        }

        let username =
            Username::new(&principal.username).map_err(|_| UserDetailError::UserNotFound {
                username: principal.username.clone(),
            })?;
        let user = self
            .user_repo
            .find_by_username(&username)
            .await
            .map_err(|err| UserDetailError::Generic(err.to_string()))?;

        if user.disabled || !self.roles.is_allowed(user.role) {
            return Err(UserDetailError::UserNotFound {
                username: principal.username.clone(),
            });
        }

        let namespace_id = self
            .namespaces
            .ensure_default_for_owner(&user.id)
            .await
            .map_err(|err| UserDetailError::Generic(err.to_string()))?;

        Ok(VfilesFtpUser {
            id: user.id,
            username: user.username.as_str().to_string(),
            role: user.role,
            namespace_id,
        })
    }
}
