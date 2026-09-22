//! WebDAV Basic 认证——复用 `AuthService::verify_credentials`（与 Web/FTP 一致）。
//! TODO(r103)：axum extractor（Authorization: Basic base64 解码 → verify_credentials →
//! 命名空间会话载体），范本 = vfiles-ftp/src/auth.rs。

#![allow(dead_code)]

/// 已认证会话（用户名 + 命名空间上下文）。
pub struct WebdavAuthenticator {
    pub username: String,
}
