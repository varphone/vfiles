//! WebDAV（RFC 4918 子集）服务端——通用文件管理生态接入面。
//!
//! 目标客户端：macOS Finder / Windows 资源管理器（映射网络驱动器）/ rclone /
//! Cyberduck / davfs2。与 vfiles-ftp 并列为第二协议面（独立端口 ✓ 独立开关 ✓
//! auth 复用 `AuthService::verify_credentials` 与 Web/FTP 完全一致）。
//!
//! 支持 OPTIONS / PROPFIND（Depth 0/1）/ GET / HEAD、PUT/DELETE/MKCOL/MOVE/COPY、
//! LOCK/UNLOCK（exclusive write、depth 0/infinity、refresh；shared lock 明确返回 405）。

mod auth;
mod lock;
mod write;

pub use lock::LockTable;
pub use write::{WebdavCopyOptions, WebdavWriteOps};
mod response;
mod server;

pub use auth::VerifyFn;
pub use auth::WebdavAuthenticator;
pub use server::{
    WebdavApplication, WebdavSettings, router_for_tests as router_for_e2e, run_webdav_server,
    spawn_webdav_server,
};
