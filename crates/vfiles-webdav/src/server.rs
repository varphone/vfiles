//! WebDAV axum 服务端（独立端口 ✓ 与 vfiles-ftp 挂载式并列）。
//!
//! TODO(r103)：OPTIONS / PROPFIND（Depth 0/1）/ GET / HEAD 只读四法（r102 定案）+
//! 写法五件（PUT/DELETE/MKCOL/MOVE/COPY）。LOCK/UNLOCK = 405（记档 ✓
//! Windows 映射依赖锁待后续评估）。
//! 存储门面 = `BackendDeps`（PROPFIND=entry_repo、GET=blob_store ✓ 范本
//! vfiles-ftp/src/backend.rs:90）。

#![allow(dead_code)]

use std::sync::Arc;

/// WebDAV 服务配置（env 对称 vfiles-ftp：VFILES_WEBDAV_ENABLED/PORT）。
#[derive(Debug, Clone)]
pub struct WebdavSettings {
    pub bind: String,
}

/// 服务共享依赖（FTP 后端 `BackendDeps` 同构持有）。
#[derive(Clone)]
pub struct WebdavApplication {
    pub username_hint: Arc<str>,
}

/// 同步运行（tokio runtime 内 ✓ 与 run_ftp_server 同式）。
/// TODO(r103)：axum::serve 监听 + 方法路由（PROPFIND 等非标 → any）。
pub async fn run_webdav_server(
    _settings: WebdavSettings,
    _app: WebdavApplication,
) -> anyhow::Result<()> {
    // r102 = 骨架编译绿；r103 实装四法 + 挂载。
    Ok(())
}

/// 后台拉起（bin 挂载用 ✓ 与 spawn_ftp_server 同签名式）。
/// TODO(r103)：spawn + shutdown channel 接线（范本 vfiles-bin main.rs:1320）。
pub fn spawn_webdav_server(
    settings: WebdavSettings,
    app: WebdavApplication,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    tokio::spawn(async move {
        let _ = shutdown.await;
        let _ = run_webdav_server(settings, app).await;
    });
    Ok(())
}
