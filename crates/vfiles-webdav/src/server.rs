//! WebDAV axum 服务端（独立端口 ✓ 与 vfiles-ftp 挂载式并列）。
//!
//! r103 中段实件：OPTIONS（Allow 头实 ✓ curl 可证）+ PROPFIND/HEAD 路由壳。
//! TODO(r104)：PROPFIND 域接线（`BackendDeps::entry_repo.find_children/find_by_path`
//! + namespace 解析链）→ GET（Entry → EntryVersion → BlobStore::get_blob_stream）→
//! 写法五件（PUT/DELETE/MKCOL/MOVE/COPY）。LOCK/UNLOCK = 405（记档 ✓
//! Windows 映射锁依赖待评估）。

#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, Method, StatusCode},
    response::Response,
    routing::any,
    Router,
};

use crate::response::PropResponse;

/// WebDAV 服务配置（env 对称 vfiles-ftp：VFILES_WEBDAV_ENABLED/PORT）。
#[derive(Debug, Clone)]
pub struct WebdavSettings {
    pub bind: String,
}

/// 服务共享依赖（r104 注入 BackendDeps + AuthService）。
#[derive(Clone)]
pub struct WebdavApplication {
    pub auth: Arc<crate::auth::WebdavAuthenticator>,
}

/// WebDAV 能力宣告（无锁 ✓ 子集 ✓）。
const ALLOW: &str = "OPTIONS, PROPFIND, GET, HEAD";

fn router() -> Router {
    Router::new().fallback(any(dav))
}

/// 方法分派（PROPFIND 等非标方法经 `any` 到达 ✓）。
async fn dav(req: axum::extract::Request) -> Response {
    match *req.method() {
        Method::OPTIONS => Response::builder()
            .status(StatusCode::OK)
            .header(header::ALLOW, ALLOW)
            .header("DAV", "1")
            .body(Body::empty())
            .unwrap(),
        // TODO(r104)：PROPFIND 域接线（multistatus body）。
        ref m if m.as_str() == "PROPFIND" => Response::builder()
            .status(StatusCode::NOT_IMPLEMENTED)
            .body(Body::from("PROPFIND 待 r104 域接线"))
            .unwrap(),
        ref m if m == "LOCK" || m == "UNLOCK" => Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())
            .unwrap(),
        _ => Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())
            .unwrap(),
    }
}

/// 同步运行（tokio runtime 内 ✓ 与 run_ftp_server 同式）。
pub async fn run_webdav_server(
    settings: WebdavSettings,
    _app: WebdavApplication,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(&settings.bind).await?;
    axum::serve(listener, router()).await?;
    Ok(())
}

/// 后台拉起（bin 挂载用 ✓ 与 spawn_ftp_server 同签名式）。
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

/// 路由构造器（测试与 run 复用 ✓）。
pub fn router_for_tests() -> Router {
    router()
}

// PropResponse 在 server 内暂未消费（r104 PROPFIND 用）——显式引用消除 dead_code 语义含混。
#[allow(unused_imports)]
use PropResponse as _PropResponseForR104;
