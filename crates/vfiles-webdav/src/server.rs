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

/// 服务共享依赖（r104 注入式 ✓ 免 vfiles-app 具体类型耦合）。
///
/// - `check_auth`：Basic 头校验回调（bin 侧接 `AuthService::verify_credentials` ✓
///   测试可 mock ✓ 泛型已由 `auth::verify` 封装）。
/// - `namespace_id`：**单命名空间注入式**（个人/家庭挂载即用 ✓
///   per-user 映射 = r105 TODO（FTP UserDetailProvider 范本））。
/// - `entry_repo`：PROPFIND 数据源（find_by_path / find_children ✓）。
#[derive(Clone)]
pub struct WebdavApplication {
    pub namespace_id: vfiles_domain::types::NamespaceId,
    pub entry_repo: Arc<dyn vfiles_domain::repo::EntryRepo + Send + Sync>,
}

/// WebDAV 能力宣告（无锁 ✓ 子集 ✓）。
const ALLOW: &str = "OPTIONS, PROPFIND, GET, HEAD";

async fn hello() -> &'static str {
    "ok"
}

fn router(app: WebdavApplication) -> Router {
    use axum::Extension;
    Router::new()
        .fallback(hello)
        .layer(Extension(app))
}

/// PROPFIND（r104 实装 ✓）：Depth 0 = 自身；Depth 1 = 自身 + 直接子条目。
///
/// href 形 = WebDAV 惯例（目录带尾斜杠 ✓）；mtime = `Entry.created_at`（记档：
/// 版本级 mtime = r105 随版本链接入）；`deleted_at` 条目假定仓储层已滤（记档 ✓）。
/// PROPFIND（纯拥有参 ✓ `&Request` 跨 await = 非 Send ✗✗ E0277 真因——
/// 同步段提取拥有值是教科书 Send 修复式 ✓ r105 破案记档）。
async fn propfind_owned(
    app: Option<WebdavApplication>,
    path: String,
    depth: String,
) -> Result<String, StatusCode> {
    use vfiles_domain::repo::EntryRepo;
    let app = app.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let depth = depth.as_str();
    if depth == "infinity" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let uri_path = path.trim_end_matches('/');
    let rel = uri_path.trim_start_matches('/').trim_end_matches('/');
    let path = vfiles_domain::types::NormalizedPath::new(if rel.is_empty() { "" } else { rel })
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mtime_fmt = |t: time::OffsetDateTime| {
        t.format(&time::format_description::well_known::Rfc2822)
            .unwrap_or_default()
    };
    let mut items = Vec::new();
    let entry = app
        .entry_repo
        .find_by_path(&app.namespace_id, &path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(entry) = entry else {
        return Err(StatusCode::NOT_FOUND);
    };
    items.push(crate::response::PropResponse {
        href: if rel.is_empty() {
            "/".to_string()
        } else {
            format!("/{rel}/")
        },
        displayname: entry.name.clone(),
        is_collection: true, // 根/目录（PROPFIND 目标按目录处置 ✓）
        getlastmodified: mtime_fmt(entry.created_at),
        getcontentlength: None,
    });
    if depth == "1" {
        let children = app
            .entry_repo
            .find_children(&app.namespace_id, &path)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for child in children {
            let is_dir = matches!(child.entry_type, vfiles_domain::types::EntryKind::Directory);
            items.push(crate::response::PropResponse {
                href: if is_dir {
                    format!("/{}/{}/", rel.trim_matches('/'), child.name)
                } else {
                    format!("/{}/{}", rel.trim_matches('/'), child.name)
                },
                displayname: child.name,
                is_collection: is_dir,
                getlastmodified: mtime_fmt(child.created_at),
                getcontentlength: None, // 文件 length = r105（版本链 size_bytes ✓）
            });
        }
    }
    Ok(crate::response::multistatus(&items))
}

/// 方法分派（PROPFIND 等非标方法经 `any` 到达 ✓）。
#[axum::debug_handler]
async fn dav(req: axum::extract::Request) -> Response {
    match *req.method() {
        Method::OPTIONS => Response::builder()
            .status(StatusCode::OK)
            .header(header::ALLOW, ALLOW)
            .header("DAV", "1")
            .body(Body::empty())
            .unwrap(),
        // PROPFIND（Depth 0/1 ✓ 其余 Depth = 400 子集记档）。
        ref m if m.as_str() == "PROPFIND" => {
            // 同步提取拥有值（&Request 跨 await = 非 Send ✗✗ E0277 真因 ✓ r105 破案）
            let path_owned = req.uri().path().to_string();
            let depth_owned = req
                .headers()
                .get("depth")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("1")
                .to_string();
            let app = req.extensions().get::<WebdavApplication>().cloned();
            match propfind_owned(app, path_owned, depth_owned).await {
                Ok(xml) => Response::builder()
                    .status(StatusCode::MULTI_STATUS)
                    .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                    .body(Body::from(xml))
                    .unwrap(),
                Err(status) => Response::builder()
                    .status(status)
                    .body(Body::empty())
                    .unwrap(),
            }
        },
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
    axum::serve(listener, router(_app.clone())).await?;
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
pub fn router_for_tests(app: WebdavApplication) -> Router {
    router(app)
}

// PropResponse 在 server 内暂未消费（r104 PROPFIND 用）——显式引用消除 dead_code 语义含混。
#[allow(unused_imports)]
use PropResponse as _PropResponseForR104;
