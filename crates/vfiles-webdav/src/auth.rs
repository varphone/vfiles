//! WebDAV Basic 认证——复用 `AuthService::verify_credentials`（与 Web/FTP 一致）。
//!
//! 实件（r103）：`basic_credentials` 解码（Base64 → (username, password)）+ 401 处置
//! 形。TODO(r104)：axum extractor 注入 AuthService + 登录限流（范本 =
//! vfiles-ftp/src/auth.rs `VfilesAuthenticator`）+ 命名空间解析（backend 侧链）。

#![allow(dead_code)]

use base64::Engine;

/// 已认证会话（用户名 + TODO(r104) 命名空间上下文）。
#[derive(Debug, Clone)]
pub struct WebdavAuthenticator {
    pub username: String,
}

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
