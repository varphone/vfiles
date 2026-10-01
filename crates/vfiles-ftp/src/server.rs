//! FTP 服务装配：监听、优雅停机、每连接会话。

use std::{net::SocketAddr, sync::Arc, time::Duration};

use libunftp::{Server, ServerBuilder};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, watch},
};
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
    /// 控制连接并发上限（包含尚未认证的连接）。
    pub max_connections: u32,
    /// 对外通告的被动模式地址（NAT/端口映射场景）；支持 IP 或域名。
    pub passive_host: Option<String>,
    pub greeting: &'static str,
    pub idle_timeout_secs: u64,
    /// FTPS 所需的 PEM 证书与私钥。
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    /// 证书和私钥由服务端生成并持久化，而不是管理员配置的证书。
    pub tls_self_signed: bool,
    /// 自动生成证书需要覆盖的主机名和 IP 地址。
    pub tls_subject_alt_names: Vec<String>,
    /// 是否强制控制通道和数据通道使用 FTPS；安全模式要求为 `true`。
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
    session_permit: Option<Arc<tokio::sync::OwnedSemaphorePermit>>,
) -> Result<Server<VfilesStorageBackend, VfilesFtpUser>, libunftp::ServerError> {
    let deps = app.backend.clone();
    let backend_permit = session_permit.clone();
    let generator: Box<dyn Fn() -> VfilesStorageBackend + Send + Sync> = Box::new(move || {
        VfilesStorageBackend::new_with_session_permit(deps.clone(), backend_permit.clone())
    });

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
            if !settings.tls_required {
                return Err(
                    std::io::Error::other("FTP 必须要求 FTPS，禁止明文或可降级连接").into(),
                );
            }
            builder = builder.ftps(cert.clone(), key.clone());
            // 控制通道及数据通道都必须加密，防止口令、命令与文件内容泄露。
            builder = builder.ftps_required(true, true);
        }
        (None, None) => {
            return Err(
                std::io::Error::other("FTP 必须配置 FTPS 证书与私钥，明文 FTP 不受支持").into(),
            );
        }
        _ => {
            return Err(std::io::Error::other("FTPS 需要同时配置证书与私钥").into());
        }
    }

    builder.build()
}

/// 启动 FTP 服务（后台任务），返回句柄。
///
/// `shutdown` 变为 `true` 时停止接受新连接，并等待现有会话清理完成。
pub async fn spawn_ftp_server(
    settings: FtpSettings,
    app: FtpApplication,
    mut shutdown: watch::Receiver<bool>,
) -> std::io::Result<FtpServerHandle> {
    if settings.tls_self_signed {
        let (Some(certificate), Some(private_key)) = (&settings.tls_cert, &settings.tls_key) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "自动生成 FTPS 证书需要证书和私钥路径",
            ));
        };
        let fingerprint = crate::certificate::ensure_self_signed_certificate(
            std::path::Path::new(certificate),
            std::path::Path::new(private_key),
            &settings.tls_subject_alt_names,
        )?;
        warn!(
            certificate,
            certificate_file_sha256 = %fingerprint,
            "已生成或复用 FTPS 自签名证书；客户端首次连接前应通过可信渠道核对证书指纹并安装信任"
        );
    }

    let listener = TcpListener::bind(settings.bind).await?;
    let local_addr = listener.local_addr()?;
    let max_connections = settings.max_connections.max(1);
    let sessions = Arc::new(Semaphore::new(max_connections as usize));

    // 预先构建一次以尽早暴露配置错误（证书等），随后按连接重建
    build_server(&settings, &app, None).map_err(|err| std::io::Error::other(err.to_string()))?;

    info!(
        address = %local_addr,
        passive_ports = ?settings.passive_ports,
        tls = settings.tls_cert.is_some(),
        "FTP 服务已启动"
    );

    let join = tokio::spawn(async move {
        let mut session_tasks = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer)) => {
                            let permit = match Arc::clone(&sessions).try_acquire_owned() {
                                Ok(permit) => permit,
                                Err(_) => {
                                    debug!(peer = %peer, max_connections, "FTP 连接数达到上限，拒绝新连接");
                                    drop(stream);
                                    continue;
                                }
                            };
                            let permit = Arc::new(permit);
                            let server = match build_server(&settings, &app, Some(Arc::clone(&permit))) {
                                Ok(server) => server,
                                Err(err) => {
                                    warn!(error = %err, "构建 FTP 会话失败");
                                    continue;
                                }
                            };
                            let session_shutdown = shutdown.clone();
                            session_tasks.spawn(async move {
                                let _permit = permit;
                                debug!(peer = %peer, "FTP 会话开始");
                                let stop_session = wait_for_shutdown(session_shutdown);
                                if let Err(err) = server.service_with_shutdown(stream, stop_session).await {
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
                Some(result) = session_tasks.join_next(), if !session_tasks.is_empty() => {
                    if let Err(err) = result {
                        warn!(error = %err, "FTP 会话任务未正常结束");
                    }
                }
            }
        }
        info!("FTP 服务停止接受新连接");
        while let Some(result) = session_tasks.join_next().await {
            if let Err(err) = result {
                warn!(error = %err, "FTP 会话任务未正常结束");
            }
        }
        info!("FTP 会话已退出，等待后台提交完成");
        let _all_session_slots = Arc::clone(&sessions)
            .acquire_many_owned(max_connections)
            .await;
        info!("FTP 会话与后台提交已全部完成");
    });

    Ok(FtpServerHandle { local_addr, join })
}

async fn wait_for_shutdown(mut shutdown: watch::Receiver<bool>) {
    loop {
        if *shutdown.borrow() || shutdown.changed().await.is_err() {
            return;
        }
    }
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
