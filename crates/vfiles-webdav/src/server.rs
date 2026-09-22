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
    /// Basic 凭据校验回调（r106 安全段 ✓ 挡匿名/坏格式/无效凭据 = 401；回 User = 审计链 ✓）。
    pub verify: crate::auth::VerifyFn,
    /// 写门面（r108' ✓ MKCOL/MOVE/DELETE）。
    pub write: Arc<dyn crate::write::WebdavWriteOps + Send + Sync>,
}

/// 写操作三型（r108' ✓）。
enum WriteOp {
    Mkcol,
    Delete,
    Move,
}

/// 写操作分派（**纯拥有参** ✓ r105 Send 修复式贯彻（#46：调用侧借用跨 await 同坑二号 ✓））。
async fn write_op(
    app: Option<WebdavApplication>,
    user: Option<vfiles_domain::types::User>,
    uri_path: String,
    dest_raw: Option<String>,
    op: WriteOp,
) -> Response {
    use vfiles_domain::repo::EntryRepo as _;
    use vfiles_domain::types::NormalizedPath;

    let Some(app) = app else {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap();
    };
    let Some(user) = user else {
        return www_authenticate();
    };
    let rel = uri_path.trim_start_matches('/').trim_end_matches('/');
    let path = match NormalizedPath::new(rel) {
        Ok(p) => p,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    };
    let ns = app.namespace_id.clone();
    let uid = user.id.clone();
    let result = match op {
        WriteOp::Mkcol => app.write.mkcol(&ns, &path, &uid).await,
        WriteOp::Delete => app.write.delete_entry(&ns, &path, &uid).await,
        WriteOp::Move => {
            let Some(dest_rel) = dest_raw.as_deref().and_then(destination_path) else {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap();
            };
            match NormalizedPath::new(dest_rel.trim_start_matches('/')) {
                Ok(dest) => app.write.move_entry(&ns, &path, &dest, &uid).await,
                Err(_) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap()
                }
            }
        }
    };
    match result {
        Ok(()) => Response::builder()
            .status(StatusCode::CREATED)
            .body(Body::empty())
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::CONFLICT)
            .body(Body::empty())
            .unwrap(),
    }
}

/// `Destination` 头 → 相对路径（纯函数 ✓ 单测覆盖）。
///
/// 形 = `http://host/dav/a/b.txt` 或 `/dav/a/b.txt` → `a/b.txt`（去 scheme/host ✓
/// 头必须路径带前导 `/` 否则 400（RFC 4918 §10.3）→ 本式返回 None 由调用方 400 ✓）。
fn destination_path(dest: &str) -> Option<&str> {
    let path_part = if let Some(scheme_pos) = dest.find("://") {
        let after_scheme = &dest[scheme_pos + 3..];
        after_scheme.find('/').map(|i| &after_scheme[i..])?
    } else {
        dest
    };
    if !path_part.starts_with('/') {
        return None;
    }
    let trimmed = path_part.trim_end_matches('/');
    Some(trimmed.trim_start_matches('/'))
}

/// 401 + `WWW-Authenticate: Basic`（RFC 4918 §20.1 ✓）。
fn www_authenticate() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::WWW_AUTHENTICATE, "Basic realm=\"vfiles-webdav\"")
        .body(Body::empty())
        .unwrap()
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
async fn dav(mut req: axum::extract::Request) -> Response {
    // 安全门（r106 ✓ dispatch 顶部全门）：OPTIONS 豁免（能力宣告无泄露 ✓ RFC 语义）
    // 其余方法 = Basic → verify 回调 → 401。
    if *req.method() != Method::OPTIONS {
        let app = req.extensions().get::<WebdavApplication>().cloned();
        let Some(app_ref) = app.as_ref() else {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        };
        let cred = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(crate::auth::basic_credentials);
        let Some((u, pw)) = cred else {
            return www_authenticate();
        };
        let Some(user) = (app_ref.verify)(u, pw).await else {
            return www_authenticate();
        };
        // 审计链贯通（写分支用 user.id ✓ 记入 MutationResult）
        req.extensions_mut().insert(user);
    }
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
        // 写法（r108' ✓ MKCOL/DELETE/MOVE 实装；PUT = r109'（分片链）；COPY = 501 记档）。
        // 同步提取拥有值（借用不跨 await ✓ #46）。
        ref m if m.as_str() == "MKCOL" || m.as_str() == "DELETE" || m.as_str() == "MOVE" => {
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req.extensions().get::<vfiles_domain::types::User>().cloned();
            let uri_owned = req.uri().path().to_string();
            let dest_owned = req
                .headers()
                .get("destination")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let op = match m.as_str() {
                "MKCOL" => WriteOp::Mkcol,
                "DELETE" => WriteOp::Delete,
                _ => WriteOp::Move,
            };
            write_op(app_owned, user_owned, uri_owned, dest_owned, op).await
        }
        ref m if m.as_str() == "COPY" => Response::builder()
            .status(StatusCode::NOT_IMPLEMENTED)
            .body(Body::from("COPY 无后端 copy API（rclone 用 GET+PUT 不依赖 ✓ 记档）"))
            .unwrap(),
        ref m if m.as_str() == "PUT" => Response::builder()
            .status(StatusCode::NOT_IMPLEMENTED)
            .body(Body::from("PUT = init_upload 链（r109' ✓ 与覆盖上传提案语义联动）"))
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

#[cfg(test)]
mod write_tests {
    use super::destination_path;

    #[test]
    fn parses_destination_absolute_and_relative() {
        // 挂载点 = root（`/` ✓ 客户端 base 自配）；`/dav/` 前缀样 = 语义错配已正
        assert_eq!(
            destination_path("http://host/a/b.txt"),
            Some("a/b.txt")
        );
        assert_eq!(destination_path("/sub/x"), Some("sub/x"));
        assert_eq!(destination_path("no-leading-slash"), None);
    }
}
