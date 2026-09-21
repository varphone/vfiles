//! FTP(S) 批量导入支持。
//!
//! 把 VFiles 的条目/版本/blob 模型暴露为一个 FTP 虚拟文件系统：
//! 客户端（FileZilla / WinSCP / `curl` / `lftp`）可以递归上传目录，
//! 服务端按 `ImportBatch` 批量提交快照，避免逐文件写全量快照。
//!
//! 模块划分：
//! - [`auth`]：复用 Web 端的凭据校验、角色白名单与登录限流；
//! - [`backend`]：`StorageBackend` 实现（list/get/put/mkd/rmd/del/rename/cwd）；
//! - [`path`]：客户端路径 → 命名空间内路径（沙箱）；
//! - [`error`]：领域错误 → FTP 应答码；
//! - [`server`]：服务装配与优雅停机。

pub mod auth;
pub mod backend;
pub mod error;
pub mod path;
pub mod server;

pub use auth::{RoleFilter, VfilesAuthenticator, VfilesFtpUser, VfilesUserDetailProvider};
pub use backend::{BackendDeps, VfilesMetadata, VfilesStorageBackend};
pub use server::{FtpApplication, FtpServerHandle, FtpSettings, run_ftp_server, spawn_ftp_server};
