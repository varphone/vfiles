//! WebDAV Basic 认证——复用 `AuthService::verify_credentials`（与 Web/FTP 一致）。
//!
//! 认证负责解析 Basic 凭据；服务分派层通过共享 `LoginAttemptLimiter` 限制失败尝试，
//! 再调用注入的校验器，以便该模块不直接依赖具体认证仓储。

use base64::Engine;

/// 已认证会话（用户名 ✓ per-user 命名空间映射 = r105 TODO（FTP UserDetailProvider 范本））。
#[derive(Debug, Clone)]
pub struct WebdavAuthenticator {
    pub username: String,
}

/// 校验回调型（bin 侧接 `AuthService::verify_credentials`）。
pub type VerifyFn = std::sync::Arc<
    dyn Fn(
            String,
            String,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Option<vfiles_domain::types::User>> + Send>,
        > + Send
        + Sync,
>;

/// 解码 `Authorization: Basic <base64>` → `(username, password)`。
///
/// 语法错/非 Basic/解码失败 → `None`（调用方回 401 + `WWW-Authenticate: Basic`）。
pub fn basic_credentials(header: &str) -> Option<(String, String)> {
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (user, pass) = text.split_once(':')?;
    if user.is_empty() {
        return None;
    }
    Some((user.to_string(), pass.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_basic_credentials() {
        // "demo:secret" 的 Base64 = ZGVtbzpzZWNyZXQ=
        assert_eq!(
            basic_credentials("Basic ZGVtbzpzZWNyZXQ="),
            Some(("demo".into(), "secret".into()))
        );
    }

    #[test]
    fn rejects_malformed_headers() {
        assert_eq!(basic_credentials("Bearer abc"), None);
        assert_eq!(basic_credentials("Basic !!!"), None);
        assert_eq!(basic_credentials("Basic Og=="), None); // ":"
    }
}
