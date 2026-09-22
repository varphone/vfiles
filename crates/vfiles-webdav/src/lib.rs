//! WebDAV（RFC 4918 子集）服务端——通用文件管理生态接入面。
//!
//! 目标客户端：macOS Finder / Windows 资源管理器（映射网络驱动器）/ rclone /
//! Cyberduck / davfs2。与 vfiles-ftp 并列为第二协议面（独立端口 ✓ 独立开关 ✓
//! auth 复用 `AuthService::verify_credentials` 与 Web/FTP 完全一致）。
//!
//! 支持（r102 = 只读四法）：OPTIONS / PROPFIND（Depth 0/1）/ GET / HEAD。
//! LOCK/UNLOCK 不支持（405 ✗ macOS/Linux/rclone 挂载不受影响 ✓ Windows 映射
//! 依赖锁 → 记档待后续轮次评估）。写法五件（PUT/DELETE/MKCOL/MOVE/COPY）= r103。

mod auth;
mod lock;
mod write;

pub use write::WebdavWriteOps;
pub use lock::LockTable;
mod response;
mod server;

pub use auth::WebdavAuthenticator;
pub use auth::VerifyFn;
pub use server::{
    WebdavApplication, WebdavSettings, run_webdav_server, router_for_tests as router_for_e2e,
    spawn_webdav_server,
};
