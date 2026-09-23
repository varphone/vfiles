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
    /// per-user 命名空间服务（r109e ✓ `ensure_default_for_owner` 映射真身 ✓
    /// 语义 = FTP UserDetailProvider 同链 ✓ 多用户隔离 ✓）。
    pub namespaces: vfiles_app::NamespaceService,
    pub entry_repo: Arc<dyn vfiles_domain::repo::EntryRepo + Send + Sync>,
    /// Basic 凭据校验回调（r106 安全段 ✓ 挡匿名/坏格式/无效凭据 = 401；回 User = 审计链 ✓）。
    pub verify: crate::auth::VerifyFn,
    /// 写门面（r108' ✓ MKCOL/MOVE/DELETE ✗ r5 +COPY）。
    pub write: Arc<dyn crate::write::WebdavWriteOps + Send + Sync>,
    /// 审计闭包（r5 ✓ 零泛型下渗 ✗ None = 不记（宽松装配）；bin 捕 AuditService spawn ✓）。
    pub audit: Option<std::sync::Arc<dyn Fn(vfiles_domain::types::NewAuditLog) + Send + Sync>>,
    /// 排他写锁表（r109a ✓ LOCK/UNLOCK + 写操作 423 校验）。
    pub locks: Arc<crate::lock::LockTable>,
}

/// `If` 头 token 提取（纯函数 ✓ 单测；复杂式（多重/嵌套）= None → 调用方 412 记档 ✓）。
fn if_token(header: &str) -> Option<String> {
    // 多重/嵌套（AND/OR）= 拒（调用方 412 记档 ✓ 简式 = 单 token 放行 ✓）
    if header.matches("opaquelocktoken:").count() > 1 {
        return None;
    }
    let start = header.find("opaquelocktoken:")?;
    let end = header[start..].find('>').map(|i| start + i)?;
    Some(header[start..end].to_string())
}

/// LOCK（r109a ✓ exclusive write / depth 0 ✓ 已锁 = 423 ✓ **纯拥有参**（#46 纪律））。
async fn lock_op(
    app: Option<WebdavApplication>,
    user: Option<vfiles_domain::types::User>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_path: String,
) -> Response {
    let Some(app) = app else {
        return internal_error();
    };
    let Some(user) = user else {
        return www_authenticate();
    };
    let Some(ns) = ns else {
        return internal_error();
    };
    let rel = uri_path.trim_start_matches('/').trim_end_matches('/').to_string();
    let lock_key = format!("{ns}:{rel}");
    match app.locks.lock(&lock_key, user.username.as_str()) {
        Some(entry) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
            .header("Lock-Token", format!("<{}>", entry.token))
            .body(Body::from(crate::response::lock_response(
                &entry.token,
                &entry.owner,
                &entry.path,
            )))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::LOCKED)
            .body(Body::empty())
            .unwrap(),
    }
}

/// UNLOCK（r109a ✓ `Lock-Token` 头匹配解 ✓ 不匹配 = 409 ✓ **纯拥有参**（#46 纪律））。
async fn unlock_op(
    app: Option<WebdavApplication>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_path: String,
    token_raw: Option<String>,
) -> Response {
    let Some(app) = app else {
        return internal_error();
    };
    let Some(ns) = ns else {
        return internal_error();
    };
    let rel = uri_path.trim_start_matches('/').trim_end_matches('/').to_string();
    let token = token_raw.and_then(|v| {
        let trimmed = v.trim().trim_start_matches('<').trim_end_matches('>');
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let Some(token) = token else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap();
    };
    let lock_key = format!("{ns}:{rel}");
    match app.locks.unlock(&lock_key, &token) {
        Some(_) => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::CONFLICT)
            .body(Body::empty())
            .unwrap(),
    }
}

fn internal_error() -> Response {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::empty())
        .unwrap()
}

/// PUT（r110'b ✓ 纯拥有参（#46 纪律）✓ 覆盖语义 = 后端版本化（呼应 PROPOSAL ✓））。
/// GET/HEAD（r110'c ✓ 纯拥有参（#46）✓ HEAD = 同头无体 ✓）。
async fn get_op(
    app: Option<WebdavApplication>,
    user: Option<vfiles_domain::types::User>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_owned: String,
    range_owned: Option<String>,
    is_head: bool,
) -> Response {
    let Some(app) = app else {
        return internal_error();
    };
    let Some(_user) = user else {
        return www_authenticate();
    };
    let Some(ns) = ns else {
        return internal_error();
    };
    let rel = uri_owned.trim_start_matches('/').trim_end_matches('/').to_string();
    let path = match vfiles_domain::types::NormalizedPath::new(&rel) {
        Ok(p) => p,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    };
    match app.write.get_stream(&ns, &path).await {
        Ok(Some((mut reader, mime, size))) => {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            // 流式响应（r201 ✓ 大文件不入内存）+ Range 分段（r211 ✓ RFC 7233 ✗
            // 此前忽略 Range = 播放器要 206 给全量 200 = mp4 循环重试真因）
            let base = Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header("accept-ranges", "bytes");
            let range = range_owned.as_deref().and_then(|rh| {
                match parse_byte_range(rh, size) {
                    ByteRange::Satisfiable(a, b) => Some(Ok((a, b))),
                    ByteRange::Unsatisfiable => Some(Err(())),
                    ByteRange::NotApplicable => None,
                }
            });
            match range {
                Some(Ok((start, end))) => {
                    // 206 分段（seek + take ✗ 仍流式 ✓ 播放器 Range 主流请求式）
                    let len = (end - start + 1) as u64;
                    if let Err(err) = reader.seek(std::io::SeekFrom::Start(start as u64)).await {
                        tracing::error!(error = %err, "WebDAV GET seek 失败");
                        return internal_error();
                    }
                    let builder = base
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(
                            header::CONTENT_RANGE,
                            format!("bytes {start}-{end}/{size}"),
                        )
                        .header(header::CONTENT_LENGTH, len.to_string());
                    if is_head {
                        builder.body(Body::empty()).unwrap()
                    } else {
                        let stream =
                            tokio_util::io::ReaderStream::new(reader.take(len));
                        builder.body(Body::from_stream(stream)).unwrap()
                    }
                }
                Some(Err(())) => Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                    .body(Body::empty())
                    .unwrap(),
                None => {
                    let builder = base
                        .status(StatusCode::OK)
                        .header(header::CONTENT_LENGTH, size.to_string());
                    if is_head {
                        builder.body(Body::empty()).unwrap()
                    } else {
                        let stream = tokio_util::io::ReaderStream::new(reader);
                        builder.body(Body::from_stream(stream)).unwrap()
                    }
                }
            }
        }
        Ok(None) => {
            // r207 观测补 ✗ 此前静默（用户"无法打开文件"无从定位 → 带路径日志）
            tracing::warn!(path = %rel, "WebDAV GET 404：路径不存在或不在当前命名空间");
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap()
        }
        Err(err) => {
            tracing::error!(path = %rel, error = %err, "WebDAV GET 读取失败（存储/版本链错误）");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap()
        }
    }
}

/// PUT（r110'b ✓ 纯拥有参（#46 四号破案））。
/// 审计统一入口（r8 ✓ COPY/PROPPATCH 十行构造式收口成一行调 ✗ audit 为 None = 跳过）。
fn audit_write(
    app: &WebdavApplication,
    action: &str,
    target: String,
    user: &vfiles_domain::types::User,
    ua: Option<&str>,
    result: vfiles_domain::types::AuditResult,
) {
    if let Some(cb) = &app.audit {
        cb(vfiles_domain::types::NewAuditLog {
            user_id: Some(user.id.clone()),
            username: user.username.as_str().to_string(),
            action: action.to_string(),
            result,
            target: Some(target),
            ip: None,
            user_agent: ua.map(ToOwned::to_owned),
            device: None,
            detail: None,
        });
    }
}

/// 写前置（r7 ✓ 锁查 + If 匹配 → 423/412 分码；None = 放行）。
fn write_precondition(
    app: &WebdavApplication,
    ns: &vfiles_domain::types::NamespaceId,
    rel: &str,
    if_header: Option<&str>,
) -> Option<StatusCode> {
    let entry = app.locks.blocked(&format!("{ns}:{rel}"))?;
    let token_ok = if_header
        .and_then(if_token)
        .map(|t| t == entry.token)
        .unwrap_or(false);
    let status = precondition_status(true, if_header.is_some(), token_ok)?;
    tracing::debug!(rel = %rel, status = %status, "写锁前置拒绝（423/412）");
    Some(status)
}

async fn put_op(
    app: Option<WebdavApplication>,
    user: Option<vfiles_domain::types::User>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_owned: String,
    if_owned: Option<String>,
    put_body: Option<Vec<u8>>,
) -> Response {
    let Some(app) = app else {
        return internal_error();
    };
    let Some(user) = user else {
        return www_authenticate();
    };
    let Some(ns) = ns else {
        return internal_error();
    };
    let Some(body_owned) = put_body else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap();
    };
    let rel = uri_owned.trim_start_matches('/').trim_end_matches('/').to_string();
    // r7 锁前置（PUT 此前零检查 ✗ 锁摆设缺口 ×1）
    if let Some(status) = write_precondition(&app, &ns, &rel, if_owned.as_deref()) {
        return Response::builder()
            .status(status)
            .body(Body::empty())
            .unwrap();
    }
    let path = match vfiles_domain::types::NormalizedPath::new(&rel) {
        Ok(p) => p,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    };
    match app.write.put_file(&ns, &path, body_owned, &user.id).await {
        Ok(()) => Response::builder()
            .status(StatusCode::CREATED)
            .body(Body::empty())
            .unwrap(),
        Err(err) => {
            tracing::warn!(path = %rel, error = %err, "WebDAV PUT 失败（409）");
            Response::builder()
                .status(StatusCode::CONFLICT)
                .body(Body::empty())
                .unwrap()
        }
    }
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
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_path: String,
    dest_raw: Option<String>,
    if_header: Option<String>,
    op: WriteOp,
) -> Response {
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
    let Some(ns) = ns else {
        return internal_error();
    };
    let rel = uri_path.trim_start_matches('/').trim_end_matches('/');
    // 写锁校验（r109a 423 → r7 分码 ✗ 有 If 不匹配 = 412（RFC §9.10.6））
    if let Some(status) = write_precondition(&app, &ns, rel, if_header.as_deref()) {
        return Response::builder()
            .status(status)
            .body(Body::empty())
            .unwrap();
    }
    let path = match NormalizedPath::new(rel) {
        Ok(p) => p,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    };
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
            // r11 分码顺修（RFC：DELETE = 204 ✗ 原三 op 全 201 = 违背顺手修 ✓）
            .status(match &op {
                WriteOp::Delete => StatusCode::NO_CONTENT,
                WriteOp::Mkcol | WriteOp::Move => StatusCode::CREATED,
            })
            .body(Body::empty())
            .unwrap(),
        Err(err) => {
            tracing::warn!(path = %rel, op = "mkcol|delete|move", error = %err, "WebDAV 写操作失败（409）");
            Response::builder()
                .status(StatusCode::CONFLICT)
                .body(Body::empty())
                .unwrap()
        }
    }
}

/// URI percent-decode（纯函数 ✓ 单测覆盖，零依赖手写 ✗ 仅解 %XX（`+` 非空格 ✗ WebDAV
/// 路径语义）→ 非法序列原样保留）——**r204 真因二号修复**：中文/空格路径直接查库
/// = 404（curl ASCII 实证从未暴露 ✗✗ 真实客户端 percent-encode 必解码）。
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

/// 条目 href 拼接（纯函数 ✓ 单测覆盖）——**前缀式**（r204 ✗ 真因修复：空 rel 时
/// `format!("/{}", "", name)` 产出双斜杠 `//x` ✗✗ = GNOME/gvfs 解析非法 href 后
/// **丢弃条目 = 列表显示为空**（curl 不校验 href ✗ 全绿假象 = 真因））。
fn entry_href(prefix: &str, name: &str, is_dir: bool) -> String {
    if is_dir {
        format!("/{prefix}{name}/")
    } else {
        format!("/{prefix}{name}")
    }
}

/// 目录前缀（根 = 空 ✗ 子目录 = `rel/`）。
fn child_prefix(rel: &str) -> String {
    let trimmed = rel.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}/")
    }
}

/// 锁前置分码（r7 ✓ RFC §4918 §9.10.6 纯函数）：
/// - 无 If 头 + 资源锁住 → 423 Locked
/// - 有 If 头但 token 不匹配 → **412 Precondition Failed**
/// - If 匹配 / 未锁 → None（放行 ✓ 未锁忽略 If = 简式记档）
fn precondition_status(locked: bool, has_if: bool, token_ok: bool) -> Option<StatusCode> {
    if !locked || token_ok {
        None
    } else if has_if {
        Some(StatusCode::PRECONDITION_FAILED)
    } else {
        Some(StatusCode::LOCKED)
    }
}

/// `Destination` 头 → 相对路径（纯函数 ✓ 单测覆盖）。
///
/// 形 = `http://host/dav/a/b.txt` 或 `/dav/a/b.txt` → `a/b.txt`（去 scheme/host ✓
/// 头必须路径带前导 `/` 否则 400（RFC 4918 §10.3）→ 本式返回 None 由调用方 400 ✓）。
fn destination_path(dest: &str) -> Option<String> {
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
    Some(percent_decode(trimmed.trim_start_matches('/')))
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
// r214 协议声明修正 ✗✗ 此前只声明 4 方法 = 实现了 10 个只报 4 个（客户端靠 Allow
// 判能力 ✗✗）；COPY/PROPPATCH 未实现不声明（声明 = 实力 ✓ 做完再加）
const ALLOW: &str = "OPTIONS, PROPFIND, PROPPATCH, GET, HEAD, PUT, DELETE, MKCOL, MOVE, COPY, LOCK, UNLOCK";

fn router(app: WebdavApplication) -> Router {
    use axum::Extension;
    Router::new()
        .fallback(dav)
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
    ns: Option<vfiles_domain::types::NamespaceId>,
    path: String,
    depth: String,
    body_owned: String,
) -> Result<String, StatusCode> {
    let app = app.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let ns = ns.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let depth = depth.as_str();
    if depth == "infinity" {
        // r2 合规修正 ✗ RFC 4918 §10.2：拒绝 infinity = 403 + DAV:propfind-finite-depth
        //（原 400 = 合规瑕疵 ✗ P1 项随手落 ✓）
        return Err(StatusCode::FORBIDDEN);
    }
    // 请求体解析（r2 P0 ✗ 非法 = 400（调用方 map_err 下述 NOT_FOUND/500 改由本处 400））
    let mode = crate::response::parse_propfind_body(&body_owned)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let uri_path = path.trim_end_matches('/');
    let rel = uri_path.trim_start_matches('/').trim_end_matches('/');
    let path = vfiles_domain::types::NormalizedPath::new(if rel.is_empty() { "" } else { rel })
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mtime_fmt = |t: time::OffsetDateTime| {
        t.format(&time::format_description::well_known::Rfc2822)
            .unwrap_or_default()
    };
    let mut items = Vec::new();
    if rel.is_empty() {
        // 根特判（RFC 4918 ✓ 空命名空间无 root Entry 行 ✗ 合成根响应 ✓）
        items.push(crate::response::PropResponse {
            href: "/".to_string(),
            displayname: "/".to_string(),
            is_collection: true,
            getlastmodified: mtime_fmt(time::OffsetDateTime::now_utc()),
            getcontentlength: None,
            getcontenttype: None,
        });
    } else {
        let entry = app
            .entry_repo
            .find_by_path(&ns, &path)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let Some(entry) = entry else {
            return Err(StatusCode::NOT_FOUND);
        };
        // r209 真因修复 ✗✗ 此前硬编码 is_collection: true = **文件被报成目录** →
        // gvfs 把文件当目录反复 PROPFIND、永不 GET = 用户"打不开文件"完整因果链
        // （列表 children 判对、查自身判错）；href 尾斜杠同错（gvfs 探了 png/ 实证）
        let is_dir = matches!(
            entry.entry_type,
            vfiles_domain::types::EntryKind::Directory
        );
        // r213 ✓ 文件大小必须给客户端（VLC/gvfs-FUSE 的 st_size 来自此属性 ✗ 缺失 =
        // 播放器视文件为空 → "无法打开 MRL" 真因嫌疑 ✗ r105 记档债在此还清 ✓
        // 单目标 = 1 次轻量 open ✓ children 批量 size = 下轮债（列表不阻塞 ✓）
        // r3：单目标双值（size + mime ✗ getcontenttype P1 顺车 ✓）
        let (getcontentlength, getcontenttype) = if is_dir {
            (None, None)
        } else {
            match app.write.get_stream(&ns, &path).await {
                Ok(Some((_reader, mime, size))) => (Some(size), Some(mime)),
                _ => (None, None),
            }
        };
        items.push(crate::response::PropResponse {
            href: if is_dir {
                format!("/{rel}/")
            } else {
                format!("/{rel}")
            },
            displayname: entry.name.clone(),
            is_collection: is_dir,
            getlastmodified: mtime_fmt(entry.created_at),
            getcontentlength,
            getcontenttype,
        });
    }
    if depth == "1" {
        // r4 批量版（N+1 消 ✗✗ 一条 SQL 直取 size/mime ✗ 替换每文件 open）
        let metas = app
            .entry_repo
            .children_with_meta(&ns, &path)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for meta in metas {
            let child = meta.entry;
            let is_dir = matches!(child.entry_type, vfiles_domain::types::EntryKind::Directory);
            let (getcontentlength, getcontenttype) = if is_dir {
                (None, None)
            } else {
                (meta.size_bytes, meta.mime_type)
            };
            items.push(crate::response::PropResponse {
                href: entry_href(&child_prefix(rel), &child.name, is_dir),
                displayname: child.name,
                is_collection: is_dir,
                getlastmodified: mtime_fmt(child.created_at),
                getcontentlength,
                getcontenttype,
            });
        }
    }
    Ok(crate::response::multistatus(&items, &mode))
}

/// Range 解析（RFC 7233 简式 ✓ 纯函数单测）。
/// - `bytes=a-b` / `bytes=a-` / `bytes=-n`（后缀）→ Some(start, end)
/// - 多段（含逗号）/ 非法 → None（调用方 = 200 全量回退，RFC 允许 ✓）
/// - 不可满足（start ≥ size 且 size > 0）→ 调用方 416（start > end 时由返回值表达）
enum ByteRange {
    Satisfiable(usize, usize),
    Unsatisfiable,
    NotApplicable,
}

fn parse_byte_range(header: &str, size: u64) -> ByteRange {
    if !header.starts_with("bytes=") || size == 0 {
        return ByteRange::NotApplicable;
    }
    let spec = &header["bytes=".len()..];
    if spec.contains(',') {
        return ByteRange::NotApplicable; // 多段 → 200 全量（简式 ✓）
    }
    let (a, b) = match spec.split_once('-') {
        Some(pair) => pair,
        None => return ByteRange::NotApplicable,
    };
    let size_i = size as usize;
    if a.is_empty() {
        // 后缀式 bytes=-n
        let n: usize = match a_is_empty_n(b) {
            Some(n) => n,
            None => return ByteRange::NotApplicable,
        };
        if n == 0 {
            return ByteRange::Unsatisfiable;
        }
        let start = size_i.saturating_sub(n);
        return ByteRange::Satisfiable(start, size_i - 1);
    }
    let start: usize = match a.parse() {
        Ok(v) => v,
        Err(_) => return ByteRange::NotApplicable,
    };
    if start >= size_i {
        return ByteRange::Unsatisfiable;
    }
    match b.parse::<usize>() {
        Ok(end) if end >= start => ByteRange::Satisfiable(start, end.min(size_i - 1)),
        Ok(_) => ByteRange::Unsatisfiable,
        Err(_) => ByteRange::Satisfiable(start, size_i - 1), // bytes=a-
    }
}

fn a_is_empty_n(suffix: &str) -> Option<usize> {
    suffix.parse::<usize>().ok()
}

/// 认证成功首行标记（r210 降噪：首条 info、其后 debug ✗ 连接可见且不刷屏）
static AUTH_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 方法分派（PROPFIND 等非标方法经 `any` 到达 ✓）。
#[axum::debug_handler]
async fn dav(mut req: axum::extract::Request) -> Response {
    // 访问日志（r208 ✓ 主流服务器标配 ✗ 此前成功连接在 info 级全隐身 = 用户"看不到日志"）
    let method = req.method().to_string();
    let path = percent_decode(req.uri().path());
    let resp = dav_inner(req).await;
    let status = resp.status().as_u16();
    if status == 404 && method == "PROPFIND" {
        // 封面/图标探测风暴（gvfs 目录内嵌封面约定 ✗ 无此文件 404 合理但刷屏 ✗ r210 降噪）
        tracing::debug!(method = %method, path = %path, status, "WebDAV 访问（探测 404 归 debug）");
    } else {
        tracing::info!(method = %method, path = %path, status, "WebDAV 访问");
    }
    resp
}

async fn dav_inner(mut req: axum::extract::Request) -> Response {
    // PUT body 预读（E0507 破案 ✓ `into_body` 需所有权 ✗ &Request ✗ = **match 前同步段**
    // 拆 owned body ✓ #46 纯拥有纪律贯彻）。
    // PUT body 预读（E0507 破案 ✓ 两步拆（#46 贯彻）：同步 take → owned to_bytes ✓）
    let put_body: Option<Vec<u8>> = if req.method() == axum::http::Method::PUT {
        let body_taken = std::mem::take(req.body_mut());
        match axum::body::to_bytes(body_taken, usize::MAX).await {
            Ok(b) => Some(b.to_vec()),
            Err(_) => {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap()
            }
        }
    } else {
        None
    };
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
            // 探测期无凭据（OPTIONS 之外首连 401 属正常流程）= debug 级别防刷屏
            tracing::debug!("WebDAV 401: 无 Authorization 头（客户端尚未发送凭据）");
            return www_authenticate();
        };
        let log_name = u.clone(); // 日志副本（u move 进 verify ✗ 先留名）
        let Some(user) = (app_ref.verify)(u, pw).await else {
            // 认证失败 = warn（用户排查关键行 ✗ 服务端日志记 username 不回客户端 ✓）
            tracing::warn!(username = %log_name, "WebDAV 认证失败（401）——检查用户名/密码，或账号是否被禁用");
            return www_authenticate();
        };
        // 首次成功 = info（连接可见 ✓）后续 debug（不每请求刷 ✗✗ r210 用户刷屏抱怨）
    if !AUTH_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::info!(username = %user.username.as_str(), "WebDAV 认证成功（本次连接后归 debug）");
    } else {
        tracing::debug!(username = %user.username.as_str(), "WebDAV 认证成功");
    }
        // per-user ns 动态映射（r109e ✓ ensure_default_for_owner ✓ 多用户隔离）
        let ns = match app_ref
            .namespaces
            .ensure_default_for_owner(&user.id)
            .await
        {
            Ok(ns) => ns,
            Err(_) => return internal_error(),
        };
        req.extensions_mut().insert(user);
        req.extensions_mut().insert(ns);
    }
    match *req.method() {
        Method::OPTIONS => Response::builder()
            .status(StatusCode::OK)
            .header(header::ALLOW, ALLOW)
            .header("DAV", "1, 2")
            .body(Body::empty())
            .unwrap(),
        // PROPFIND（Depth 0/1 ✓ 其余 Depth = 400 子集记档）。
        // GET/HEAD（r110'c ✓ 读面终件）。
        ref m if m.as_str() == "GET" || m.as_str() == "HEAD" => {
            // 纯拥有参（#46）：调用侧同步提取。
            let is_head = m.as_str() == "HEAD";
            let range_owned = req
                .headers()
                .get("range")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req.extensions().get::<vfiles_domain::types::User>().cloned();
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let uri_owned = percent_decode(req.uri().path());
            get_op(app_owned, user_owned, ns_owned, uri_owned, range_owned, is_head).await
        }
        ref m if m.as_str() == "PROPFIND" => {
            // 同步提取拥有值（&Request 跨 await = 非 Send ✗✗ E0277 真因 ✓ r105 破案）
            let path_owned = percent_decode(req.uri().path());
            let depth_owned = req
                .headers()
                .get("depth")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("1")
                .to_string();
            let app = req.extensions().get::<WebdavApplication>().cloned();
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let body_owned = {
                // #46 同步 take 转 owned（await 前结束借 ✓）
                let taken = std::mem::take(req.body_mut());
                match axum::body::to_bytes(taken, usize::MAX).await {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    Err(_) => String::new(),
                }
            };
            match propfind_owned(app, ns_owned, path_owned, depth_owned, body_owned).await {
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
        // LOCK/UNLOCK（r109a ✓ 商业级核心件 = Windows 映射依赖）。
        // 纯拥有参（#46 三号实录强化 ✗✗ 借用不跨 await = 编码模板纪律）。
        ref m if m.as_str() == "LOCK" || m.as_str() == "UNLOCK" => {
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req.extensions().get::<vfiles_domain::types::User>().cloned();
            let uri_owned = percent_decode(req.uri().path());
            let token_owned = req
                .headers()
                .get("lock-token")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let resp = if m.as_str() == "LOCK" {
                lock_op(app_owned, user_owned, ns_owned, uri_owned).await
            } else {
                unlock_op(app_owned, ns_owned, uri_owned, token_owned).await
            };
            if resp.status().is_success() {
                if let (Some(app), Some(u)) = (
                    req.extensions().get::<WebdavApplication>(),
                    req.extensions().get::<vfiles_domain::types::User>(),
                ) {
                    let action = if m.as_str() == "LOCK" { "webdav.lock" } else { "webdav.unlock" };
                    let ua = req
                        .headers()
                        .get("user-agent")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    audit_write(
                        app,
                        action,
                        percent_decode(req.uri().path()),
                        u,
                        ua.as_deref(),
                        vfiles_domain::types::AuditResult::Success,
                    );
                }
            }
            resp
        }
        // 写法（r108' ✓ MKCOL/DELETE/MOVE 实装；PUT = r109'（分片链）；COPY = 501 记档）。
        // 同步提取拥有值（借用不跨 await ✓ #46）。
        ref m if m.as_str() == "PROPPATCH" => {
            // r6 PROPPATCH（RFC 4918 §9.2 ✓ propertyupdate 解析（roxmltree）+ 每操作
            // propstat（200/403）✗ 可写集 = displayname（set → move 同父改名（r5 单源
            // 直路径语义复用 ✓）；remove 恒 403（属性不可删）✗ 其余属性 403（403 =
            // RFC §9.2.1 合规拒码 ✓）；自定义属性持久化 = P1 记债）。
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req.extensions().get::<vfiles_domain::types::User>().cloned();
            let uri_owned = percent_decode(req.uri().path());
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let body_owned = {
                let taken = std::mem::take(req.body_mut());
                match axum::body::to_bytes(taken, usize::MAX).await {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    Err(_) => String::new(),
                }
            };
            let (app_ref, user, ns) = match (app_owned, user_owned, ns_owned) {
                (Some(a), Some(u), Some(n)) => (a, u, n),
                _ => return internal_error(),
            };
            let if_owned = req
                .headers()
                .get("if")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let src_rel = uri_owned.trim_start_matches('/').to_string();
            let path = match vfiles_domain::types::NormalizedPath::new(&src_rel) {
                Ok(p) => p,
                Err(_) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            // r7 锁前置（PROPPATCH 此前零检查 ✗ 摆设缺口 ×3）
            if let Some(status) =
                write_precondition(&app_ref, &ns, path.as_str(), if_owned.as_deref())
            {
                return Response::builder()
                    .status(status)
                    .body(Body::empty())
                    .unwrap();
            }
            let ops = match crate::response::parse_propertyupdate(&body_owned) {
                Ok(ops) => ops,
                Err(()) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            // 可写集判定 + displayname 同父改名（多 set 顺序执行；名字含 / = 403 拒）
            let mut results: Vec<(crate::response::PropOp, bool)> = Vec::new();
            let mut rename_failed = false;
            for op in ops {
                match &op {
                    crate::response::PropOp::Set { name, value }
                        if name == "displayname"
                            && !value.is_empty()
                            && !value.contains('/')
                            && !rename_failed =>
                    {
                        let parent = match path.as_str().rfind('/') {
                            Some(i) => &path.as_str()[..i],
                            None => "",
                        };
                        let new_rel = if parent.is_empty() {
                            value.clone()
                        } else {
                            format!("{parent}/{value}")
                        };
                        match vfiles_domain::types::NormalizedPath::new(&new_rel) {
                            Ok(dest) => {
                                match app_ref
                                    .write
                                    .move_entry(&ns, &path, &dest, &user.id)
                                    .await
                                {
                                    Ok(()) => {
                                        // 改名成功后后续 op 的 path 同步（顺序语义 ✓）
                                        results.push((op, true));
                                        // 注：单请求多 set 改名 = 后续仍以原 path 改
                                        // （RFC 允许实现限制 ✗ 记档）
                                    }
                                    Err(err) => {
                                        tracing::warn!(error = %err, "PROPPATCH displayname 改名失败（403）");
                                        results.push((op, false));
                                        rename_failed = true;
                                    }
                                }
                            }
                            Err(_) => {
                                results.push((op, false));
                                rename_failed = true;
                            }
                        }
                    }
                    _ => results.push((op, false)),
                }
            }
            if let Some(cb) = &app_ref.audit {
                cb(vfiles_domain::types::NewAuditLog {
                    user_id: Some(user.id.clone()),
                    username: user.username.as_str().to_string(),
                    action: "webdav.proppatch".to_string(),
                    result: vfiles_domain::types::AuditResult::Success,
                    target: Some(path.as_str().to_string()),
                    ip: None,
                    user_agent: req
                        .headers()
                        .get("user-agent")
                        .and_then(|v| v.to_str().ok())
                        .map(ToOwned::to_owned),
                    device: None,
                    detail: None,
                });
            }
            let xml = crate::response::proppatch_multistatus(path.as_str(), &results);
            Response::builder()
                .status(StatusCode::MULTI_STATUS)
                .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                .body(Body::from(xml))
                .unwrap()
        }
        ref m if m.as_str() == "COPY" => {
            let _ = m;
            // r5 COPY（RFC 4918 §9.3 ✓ 同臂 owned 提取式（三合一臂照抄 ✗ #46）
            // dst 存在 → 409（Overwrite 完整覆盖 = blob release 链 P1 记债 ✗ 不假覆盖）
            // ✗ 审计闭包记 src→dst（copy 带审计链 ✓ 其余写方法接 = P0 债入基线）。
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req.extensions().get::<vfiles_domain::types::User>().cloned();
            let uri_owned = percent_decode(req.uri().path());
            let dest_hdr = req
                .headers()
                .get("destination")
                .and_then(|v| v.to_str().ok())
                .and_then(destination_path);
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let (app_ref, user, ns) = match (app_owned, user_owned, ns_owned) {
                (Some(a), Some(u), Some(n)) => (a, u, n),
                _ => return internal_error(),
            };
            let dest_hdr = match dest_hdr {
                Some(d) => d,
                None => {
                    tracing::warn!(uri = %req.uri(), "COPY 400：Destination 头缺失或不可解析");
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            let dest_path = match vfiles_domain::types::NormalizedPath::new(&dest_hdr) {
                Ok(d) => d,
                Err(err) => {
                    tracing::warn!(raw = %dest_hdr, error = %err, "COPY 400：Destination 路径非法");
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            let if_owned = req
                .headers()
                .get("if")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            // r10 Overwrite 语义（RFC 缺省 = T ✗ 只有显式 "F" 才是 false）
            let overwrite = req
                .headers()
                .get("overwrite")
                .and_then(|v| v.to_str().ok())
                != Some("F");
            // 源路径裁前导斜杠（PROPFIND 同式 ✗ 真因：new 不收前导 / ✗ 诊断日志定案 ✓）
            let src_rel = uri_owned.trim_start_matches('/').to_string();
            // r7 锁前置：源 + 目标双查（COPY 此前零检查 ✗ 摆设缺口 ×2）
            for check_rel in [
                src_rel.as_str(),
                dest_hdr.trim_matches('/'),
            ] {
                if let Some(status) =
                    write_precondition(&app_ref, &ns, check_rel, if_owned.as_deref())
                {
                    return Response::builder()
                        .status(status)
                        .body(Body::empty())
                        .unwrap();
                }
            }
            let path = match vfiles_domain::types::NormalizedPath::new(&src_rel) {
                Ok(p) => p,
                Err(err) => {
                    tracing::warn!(raw = %src_rel, error = %err, "COPY 400：源路径非法");
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            let user_id = user.id.clone();
            let username = user.username.as_str().to_string();
            let dst_existed = app_ref
                .entry_repo
                .find_by_path(&ns, &dest_path)
                .await
                .ok()
                .flatten()
                .is_some();
            // Overwrite: F + 目标存在 → 412（RFC §9.3.3 ✗ r5 曾全 409 = 违背修正）
            if dst_existed && !overwrite {
                return Response::builder()
                    .status(StatusCode::PRECONDITION_FAILED)
                    .body(Body::empty())
                    .unwrap();
            }
            match app_ref.write.copy_entry(&ns, &path, &dest_path, &user.id, overwrite).await {
                Ok(()) => {
                    if let Some(cb) = &app_ref.audit {
                        cb(vfiles_domain::types::NewAuditLog {
                            user_id: Some(user_id),
                            username,
                            action: "webdav.copy".to_string(),
                            result: vfiles_domain::types::AuditResult::Success,
                            target: Some(format!("{} -> {}", path.as_str(), dest_hdr)),
                            ip: None,
                            user_agent: req
                                .headers()
                                .get("user-agent")
                                .and_then(|v| v.to_str().ok())
                                .map(ToOwned::to_owned),
                            device: None,
                            detail: None,
                        });
                    }
                    // r10 覆盖成功 = 204（RFC §9.3.3 ✓）/ 新建 = 201
                    let status = if dst_existed {
                        StatusCode::NO_CONTENT
                    } else {
                        StatusCode::CREATED
                    };
                    Response::builder()
                        .status(status)
                        .body(Body::empty())
                        .unwrap()
                }
                Err(vfiles_domain::DomainError::Conflict { message }) => {
                    tracing::warn!(target = %dest_hdr, reason = %message, "WebDAV COPY 冲突（409）");
                    Response::builder()
                        .status(StatusCode::CONFLICT)
                        .body(Body::empty())
                        .unwrap()
                }
                Err(err) => {
                    tracing::error!(error = %err, "WebDAV COPY 失败");
                    internal_error()
                }
            }
        }
        ref m if m.as_str() == "MKCOL" || m.as_str() == "DELETE" || m.as_str() == "MOVE" => {
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req.extensions().get::<vfiles_domain::types::User>().cloned();
            let uri_owned = percent_decode(req.uri().path());
            let dest_owned = req
                .headers()
                .get("destination")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let if_owned = req
                .headers()
                .get("if")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let op = match m.as_str() {
                "MKCOL" => WriteOp::Mkcol,
                "DELETE" => WriteOp::Delete,
                _ => WriteOp::Move,
            };
            let audit_action = match m.as_str() {
                "MKCOL" => "webdav.mkcol",
                "DELETE" => "webdav.delete",
                _ => "webdav.move",
            };
            {
                let ua = req
                    .headers()
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                // r11 MOVE Overwrite（臂层式 ✗ 零签名变 ✓ extensions 重取 = 不碰已 move 变量）
                let mut move_overwrite_204 = false;
                if matches!(op, WriteOp::Move) {
                    let overwrite = req
                        .headers()
                        .get("overwrite")
                        .and_then(|v| v.to_str().ok())
                        != Some("F");
                    if let (Some(app), Some(u), Some(ns_ext)) = (
                        req.extensions().get::<WebdavApplication>(),
                        req.extensions().get::<vfiles_domain::types::User>(),
                        req.extensions().get::<vfiles_domain::types::NamespaceId>(),
                    ) {
                        if let Some(dest_rel) = dest_owned.as_deref().and_then(destination_path) {
                            // r12 dest 即 target（WebDAV 完整目标路径 ✗ r11 曾 join 目录
                            // = 违 RFC 二义 → 服务参数化后臂层同步简化 ✓ 同名目录覆盖打通）
                            if let Ok(target) = vfiles_domain::types::NormalizedPath::new(&dest_rel)
                            {
                                let target_exists = app
                                    .entry_repo
                                    .find_by_path(ns_ext, &target)
                                    .await
                                    .ok()
                                    .flatten()
                                    .is_some();
                                if target_exists {
                                    if !overwrite {
                                        // Overwrite: F + 目标存在 → 412（同 COPY r10 语义）
                                        return Response::builder()
                                            .status(StatusCode::PRECONDITION_FAILED)
                                            .body(Body::empty())
                                            .unwrap();
                                    }
                                    // T = 删旧（write.delete_entry = delete_entries 全链 = 递归 + blob release ✓）
                                    if let Err(err) = app.write.delete_entry(ns_ext, &target, &u.id).await
                                    {
                                        tracing::error!(error = %err, "MOVE 覆盖删旧失败");
                                        return internal_error();
                                    }
                                    move_overwrite_204 = true;
                                }
                            }
                        }
                    }
                }
                let resp = write_op(app_owned, user_owned, ns_owned, uri_owned, dest_owned, if_owned, op).await;
                // r11 覆盖成功 204（RFC §9.9.3 ✗ 新建保持 201）——外层改写避免 move 后外尾用旧绑
                let resp = if move_overwrite_204 && resp.status() == StatusCode::CREATED {
                    Response::builder()
                        .status(StatusCode::NO_CONTENT)
                        .body(Body::empty())
                        .unwrap()
                } else {
                    resp
                };
                if resp.status().is_success() {
                    if let (Some(app), Some(u)) = (
                        req.extensions().get::<WebdavApplication>(),
                        req.extensions().get::<vfiles_domain::types::User>(),
                    ) {
                        audit_write(
                            app,
                            audit_action,
                            percent_decode(req.uri().path()),
                            u,
                            ua.as_deref(),
                            vfiles_domain::types::AuditResult::Success,
                        );
                    }
                }
                resp
            }
        }
        ref m if m.as_str() == "COPY" => Response::builder()
            .status(StatusCode::NOT_IMPLEMENTED)
            .body(Body::from("COPY 无后端 copy API（rclone 用 GET+PUT 不依赖 ✓ 记档）"))
            .unwrap(),
        // PUT（r110'b ✓ 商业级写面终件 = 流式直传）。
        // PUT（r110'b ✓ 商业级写面终件 = 流式直传）。
        // 纯拥有参（#46 四号 ✗✗✗ 调用侧同步提取）。
        ref m if m.as_str() == "PUT" => {
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req.extensions().get::<vfiles_domain::types::User>().cloned();
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let uri_owned = percent_decode(req.uri().path());
            {
                let ua = req.headers().get("user-agent").and_then(|v| v.to_str().ok()).map(str::to_string);
                let resp = put_op(
                app_owned,
                user_owned,
                ns_owned,
                uri_owned,
                req.headers()
                    .get("if")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string),
                put_body,
            )
            .await;
                if resp.status().is_success() {
                    if let (Some(app), Some(u)) = (
                        req.extensions().get::<WebdavApplication>(),
                        req.extensions().get::<vfiles_domain::types::User>(),
                    ) {
                        audit_write(app, "webdav.put", percent_decode(req.uri().path()), u, ua.as_deref(), vfiles_domain::types::AuditResult::Success);
                    }
                }
                resp
            }
        }
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
    tracing::info!(addr = %settings.bind, "WebDAV 监听就绪（真实绑定成功后打此行；端口被占用则见 spawn error 日志）");
    axum::serve(listener, router(_app.clone())).await?;
    Ok(())
}

/// 后台拉起（bin 挂载用 ✓ 与 spawn_ftp_server 同签名式）。
pub fn spawn_webdav_server(
    settings: WebdavSettings,
    app: WebdavApplication,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    // select 并发（r110'a 破案 ✓ **顺序 bug = run 永不执行**（r103 骨架 ✗✗✗ 盲区五轮））
    tokio::spawn(async move {
        tokio::select! {
            res = run_webdav_server(settings, app) => {
                if let Err(err) = res {
                    // r205 真因修复 ✗✗ 此前 `let _ =` 吞 bind 错 = 起不来也"已启用" = 用户无从排查
                    tracing::error!(error = %err, "WebDAV 绑定/服务失败（端口被占用或地址非法）");
                }
            }
            _ = shutdown.wait_for(|stop| *stop) => {}
        }
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
            Some("a/b.txt".to_string())
        );
        assert_eq!(destination_path("/sub/x"), Some("sub/x".to_string()));
        assert_eq!(destination_path("no-leading-slash"), None);
    }
}

#[cfg(test)]
mod decode_tests {
    use super::percent_decode;

    #[test]
    fn decodes_percent_encoded_paths() {
        // r204 真因二号回归守护 ✗✗ 中文/空格必须解码（否则查库 404）
        assert_eq!(percent_decode("/%E9%A1%B9%E7%9B%AE%E5%BA%93/"), "/项目库/");
        assert_eq!(percent_decode("/a%20b.txt"), "/a b.txt");
        assert_eq!(percent_decode("/ascii/x.txt"), "/ascii/x.txt");
    }

    #[test]
    fn falls_back_on_invalid_sequences() {
        assert_eq!(percent_decode("%ZZ%"), "%ZZ%");
        assert_eq!(percent_decode("%E4%B8"), "%E4%B8"); // 截断 UTF-8 → 保留原文
    }
}

mod href_tests {
    use super::{child_prefix, entry_href};

    #[test]
    fn root_children_have_single_slash() {
        // r204 真因回归守护 ✗✗ 空 rel 必须单斜杠（//x = gvfs 丢弃条目）
        assert_eq!(entry_href(&child_prefix(""), "根文件.txt", false), "/根文件.txt");
        assert_eq!(entry_href(&child_prefix(""), "项目库", true), "/项目库/");
        assert!(!entry_href(&child_prefix(""), "x", false).starts_with("//"));
    }

    #[test]
    fn nested_children_keep_prefix() {
        assert_eq!(entry_href(&child_prefix("项目库"), "说明.md", false), "/项目库/说明.md");
        assert_eq!(entry_href(&child_prefix("项目库/子"), "a", true), "/项目库/子/a/");
    }
}

mod if_token_tests {
    use super::if_token;

    #[test]
    fn extracts_opaque_token_and_rejects_nested() {
        assert_eq!(
            if_token("(<opaquelocktoken:abc123>)"),
            Some("opaquelocktoken:abc123".into())
        );
        assert_eq!(if_token("(<opaquelocktoken:a> AND <opaquelocktoken:b>)"), None);
        assert_eq!(if_token("<Not-a-lock-token>"), None);
    }
}

#[cfg(test)]
mod range_tests {
    use super::{parse_byte_range, ByteRange};

    #[test]
    fn parses_range_forms() {
        // RFC 7233 三式 + 回退 + 不可满足（r211 播放器 Range 真因守护）
        assert!(matches!(
            parse_byte_range("bytes=0-100", 1000),
            ByteRange::Satisfiable(0, 100)
        ));
        assert!(matches!(
            parse_byte_range("bytes=500-", 1000),
            ByteRange::Satisfiable(500, 999)
        ));
        assert!(matches!(
            parse_byte_range("bytes=-100", 1000),
            ByteRange::Satisfiable(900, 999)
        ));
        // 多段 / 非法 → 200 全量回退
        assert!(matches!(
            parse_byte_range("bytes=0-1,5-6", 1000),
            ByteRange::NotApplicable
        ));
        assert!(matches!(
            parse_byte_range("digits", 1000),
            ByteRange::NotApplicable
        ));
        // 不可满足 → 416
        assert!(matches!(
            parse_byte_range("bytes=2000-", 1000),
            ByteRange::Unsatisfiable
        ));
        // 空文件 → 全量
        assert!(matches!(
            parse_byte_range("bytes=0-10", 0),
            ByteRange::NotApplicable
        ));
    }
}

#[cfg(test)]
mod precondition_tests {
    use super::write_precondition;
    use axum::http::StatusCode;
    use crate::server::precondition_status;

    #[test]
    fn codes_per_rfc4918() {
        // r7 分码守护（RFC §9.10.6）：无If锁住=423 / 有If不匹配=412 / 匹配或未锁=放行
        assert_eq!(precondition_status(true, false, false), Some(StatusCode::LOCKED));
        assert_eq!(precondition_status(true, true, false), Some(StatusCode::PRECONDITION_FAILED));
        assert_eq!(precondition_status(true, true, true), None);
        assert_eq!(precondition_status(false, false, false), None);
        assert_eq!(precondition_status(false, true, false), None);
        let _ = write_precondition; // 桥引用防空（单测走纯函数面）
    }
}
