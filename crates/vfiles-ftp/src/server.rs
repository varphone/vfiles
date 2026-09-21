//! FTP 服务装配：监听、优雅停机、每连接会话。

use std::{net::SocketAddr, sync::Arc, time::Duration};

use libunftp::{Server, ServerBuilder};
use tokio::{net::TcpListener, sync::watch};
use tracing::{debug, info, warn};
use unftp_core::auth::{Authenticator, UserDetailProvider};

use crate::auth::{VfilesAuthenticator, VfilesFtpUser, VfilesUserDetailProvider};
use crate::backend::{BackendDeps, VfilesStorageBackend};

/// 运行参数（由配置层转换而来）。
#[derive(Debug, Clone)]
pub struct FtpSettings {
    pub bind: SocketAddr,
    /// 被动模式端口段（含两端）。
    pub passive_ports: (u16, u16),
    /// 对外通告的被动模式地址（NAT/端口映射场景）；支持 IP 或域名。
    pub passive_host: Option<String>,
    pub greeting: &'static str,
    pub idle_timeout_secs: u64,
    /// PEM 证书与私钥；两者都有时启用 FTPS。
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    /// 是否拒绝明文连接（仅在启用 FTPS 时有意义）。
    pub tls_required: bool,
}

/// 认证与存储依赖。
#[derive(Clone)]
pub struct FtpApplication {
    pub backend: BackendDeps,
    pub authenticator: Arc<VfilesAuthenticator>,
    pub user_detail_provider: Arc<VfilesUserDetailProvider>,
}

impl std::fmt::Debug for FtpApplication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FtpApplication")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

/// 运行中的 FTP 服务句柄。
pub struct FtpServerHandle {
    local_addr: SocketAddr,
    join: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for FtpServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FtpServerHandle")
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

impl FtpServerHandle {
    /// 实际绑定的地址（配置端口为 0 时可用于获取随机端口）。
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 等待服务任务结束。
    pub async fn wait(self) {
        let _ = self.join.await;
    }
}

/// 每次连接构造一个 `Server`：`Server::service` 按值消费自身，
/// 而构造过程只涉及 `Arc` 克隆，代价可忽略。
fn build_server(
    settings: &FtpSettings,
    app: &FtpApplication,
) -> Result<Server<VfilesStorageBackend, VfilesFtpUser>, libunftp::ServerError> {
    let deps = app.backend.clone();
    let generator: Box<dyn Fn() -> VfilesStorageBackend + Send + Sync> =
        Box::new(move || VfilesStorageBackend::new(deps.clone()));

    let provider = Arc::clone(&app.user_detail_provider)
        as Arc<dyn UserDetailProvider<User = VfilesFtpUser> + Send + Sync>;
    let mut builder = ServerBuilder::with_user_detail_provider(generator, provider)
        .authenticator(Arc::clone(&app.authenticator) as Arc<dyn Authenticator + Send + Sync>)
        // 只允许被动模式：主动模式需要服务端回连客户端，云/内网环境基本不可用
        .active_passive_mode(libunftp::options::ActivePassiveMode::PassiveOnly)
        .passive_ports(settings.passive_ports.0..=settings.passive_ports.1)
        .greeting(settings.greeting)
        .idle_session_timeout(settings.idle_timeout_secs);

    if let Some(host) = &settings.passive_host {
        builder = builder.passive_host(host.as_str());
    }

    match (&settings.tls_cert, &settings.tls_key) {
        (Some(cert), Some(key)) => {
            builder = builder.ftps(cert.clone(), key.clone());
            if settings.tls_required {
                builder = builder.ftps_required(true, true);
            }
        }
        (None, None) => {
            if settings.tls_required {
                return Err(std::io::Error::other(
                    "VFILES_FTP_TLS_REQUIRED=true 但未配置证书与私钥",
                )
                .into());
            }
            warn!("FTP 未启用 TLS：凭据与数据均为明文，建议仅在可信内网使用");
        }
        _ => {
            return Err(std::io::Error::other("FTPS 需要同时配置证书与私钥").into());
        }
    }

    builder.build()
}

/// 启动 FTP 服务（后台任务），返回句柄。
///
/// `shutdown` 变为 `true` 时停止接受新连接；已在传输的会话自行结束。
pub async fn spawn_ftp_server(
    settings: FtpSettings,
    app: FtpApplication,
    mut shutdown: watch::Receiver<bool>,
) -> std::io::Result<FtpServerHandle> {
    let listener = TcpListener::bind(settings.bind).await?;
    let local_addr = listener.local_addr()?;

    // 预先构建一次以尽早暴露配置错误（证书等），随后按连接重建
    build_server(&settings, &app).map_err(|err| std::io::Error::other(err.to_string()))?;

    info!(
        address = %local_addr,
        passive_ports = ?settings.passive_ports,
        tls = settings.tls_cert.is_some(),
        "FTP 服务已启动"
    );

    let join = tokio::spawn(async move {
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer)) => {
                            let server = match build_server(&settings, &app) {
                                Ok(server) => server,
                                Err(err) => {
                                    warn!(error = %err, "构建 FTP 会话失败");
                                    continue;
                                }
                            };
                            tokio::spawn(async move {
                                debug!(peer = %peer, "FTP 会话开始");
                                if let Err(err) = server.service(stream).await {
                                    debug!(peer = %peer, error = %err, "FTP 会话结束（异常）");
                                }
                            });
                        }
                        Err(err) => {
                            warn!(error = %err, "FTP 监听接受连接失败");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
            }
        }
        info!("FTP 服务停止接受新连接");
    });

    Ok(FtpServerHandle { local_addr, join })
}

/// 阻塞式运行：直到 `shutdown` 触发后返回。
pub async fn run_ftp_server(
    settings: FtpSettings,
    app: FtpApplication,
    shutdown: watch::Receiver<bool>,
) -> std::io::Result<()> {
    let handle = spawn_ftp_server(settings, app, shutdown).await?;
    handle.wait().await;
    Ok(())
}
