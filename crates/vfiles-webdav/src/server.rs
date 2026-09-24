//! WebDAV axum 服务端：读写方法、锁处理、认证与协议错误语义。

#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Method, StatusCode, header},
    response::Response,
};
use http_body::Body as _;

use crate::response::PropResponse;

const MAX_DAV_XML_BODY_BYTES: usize = 1024 * 1024;

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
    /// Shared failed-login limiter used by HTTP and FTP authentication.
    pub login_attempt_limiter: Arc<vfiles_app::LoginAttemptLimiter>,
    pub login_rate_limit: vfiles_app::RateLimitPolicy,
    pub ingest_stats: Arc<vfiles_app::IngestStats>,
    /// 写门面（r108' ✓ MKCOL/MOVE/DELETE ✗ r5 +COPY）。
    pub write: Arc<dyn crate::write::WebdavWriteOps + Send + Sync>,
    /// 审计闭包（r5 ✓ 零泛型下渗 ✗ None = 不记（宽松装配）；bin 捕 AuditService spawn ✓）。
    pub audit: Option<std::sync::Arc<dyn Fn(vfiles_domain::types::NewAuditLog) + Send + Sync>>,
    /// r-new 共端口挂载前缀（"" = 独立端口现行为 ✗ "/dav" = 嵌入主端口：href 加前缀 /
    /// Destination 剥前缀 / spawn 分支按此分流 ✓ 归一无尾斜杠）。
    pub mount_prefix: String,
    /// Maximum accepted WebDAV PUT body size, shared with the HTTP upload limit.
    pub max_file_size_bytes: u64,
    /// 排他写锁表（r109a ✓ LOCK/UNLOCK + 写操作 423 校验）。
    pub locks: Arc<crate::lock::LockTable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IfCondition {
    Token { value: String, negated: bool },
    EntityTag { value: String, negated: bool },
}

#[derive(Debug, PartialEq, Eq)]
enum IfHeader {
    Untagged(Vec<Vec<IfCondition>>),
    Tagged(Vec<(String, Vec<Vec<IfCondition>>)>),
}

/// Parse either RFC 4918 `If` form. Conditions within a list are ANDed and
/// lists associated with the same resource are alternatives.
fn parse_if_header(header: &str) -> Option<IfHeader> {
    let mut remaining = header.trim();
    if remaining.starts_with('(') {
        let lists = parse_if_lists(&mut remaining)?;
        return remaining.is_empty().then_some(IfHeader::Untagged(lists));
    }

    let mut tagged = Vec::new();
    while !remaining.is_empty() {
        remaining = remaining.strip_prefix('<')?;
        let end = remaining.find('>')?;
        let resource_tag = remaining[..end].to_string();
        if resource_tag.is_empty() || resource_tag.chars().any(char::is_whitespace) {
            return None;
        }
        remaining = remaining[end + 1..].trim_start();
        if !remaining.starts_with('(') {
            return None;
        }
        let lists = parse_if_lists(&mut remaining)?;
        tagged.push((resource_tag, lists));
    }
    (!tagged.is_empty()).then_some(IfHeader::Tagged(tagged))
}

/// Parse the single RFC 4918 Overwrite field. It defaults to `T`; malformed
/// values and repeated field lines are rejected instead of silently choosing one.
fn parse_overwrite_header(headers: &axum::http::HeaderMap) -> Option<bool> {
    let mut values = headers.get_all("overwrite").iter();
    match (values.next(), values.next()) {
        (None, None) => Some(true),
        (Some(value), None) => {
            let value = value.to_str().ok()?;
            if value.eq_ignore_ascii_case("T") {
                Some(true)
            } else if value.eq_ignore_ascii_case("F") {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_write_depth(headers: &axum::http::HeaderMap) -> Result<Option<bool>, ()> {
    let mut values = headers.get_all("depth").iter();
    match (values.next(), values.next()) {
        (None, None) => Ok(None),
        (Some(value), None) => {
            let value = value.to_str().map_err(|_| ())?.trim();
            if value.eq_ignore_ascii_case("infinity") {
                Ok(Some(true))
            } else if value == "0" || value == "1" {
                Ok(Some(false))
            } else {
                Err(())
            }
        }
        _ => Err(()),
    }
}

async fn request_body_is_nonempty(body: &mut Body) -> bool {
    let size_hint = body.size_hint();
    if size_hint.lower() > 0 || size_hint.exact().is_some_and(|length| length > 0) {
        return true;
    }
    match axum::body::to_bytes(std::mem::take(body), 1).await {
        Ok(bytes) => !bytes.is_empty(),
        // An unreadable/oversized body cannot be accepted as an empty MKCOL request.
        Err(_) => true,
    }
}

fn parse_if_lists(input: &mut &str) -> Option<Vec<Vec<IfCondition>>> {
    let mut lists = Vec::new();
    while input.starts_with('(') {
        let after_open = input.strip_prefix('(')?;
        let close = after_open.find(')')?;
        let mut conditions_input = after_open[..close].trim();
        let mut conditions = Vec::new();
        while !conditions_input.is_empty() {
            let negated = if conditions_input
                .get(..3)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Not"))
                && conditions_input
                    .get(3..)
                    .and_then(|rest| rest.chars().next())
                    .is_some_and(char::is_whitespace)
            {
                conditions_input = conditions_input[3..].trim_start();
                true
            } else {
                false
            };
            if let Some(rest) = conditions_input.strip_prefix('<') {
                let end = rest.find('>')?;
                let value = &rest[..end];
                if !valid_state_token(value) {
                    return None;
                }
                conditions.push(IfCondition::Token {
                    value: value.to_string(),
                    negated,
                });
                conditions_input = rest[end + 1..].trim_start();
            } else {
                let rest = conditions_input.strip_prefix('[')?;
                let end = rest.find(']')?;
                let value = &rest[..end];
                if value != value.trim() || !valid_entity_tag(value) {
                    return None;
                }
                conditions.push(IfCondition::EntityTag {
                    value: value.to_string(),
                    negated,
                });
                conditions_input = rest[end + 1..].trim_start();
            }
        }
        if conditions.is_empty() {
            return None;
        }
        lists.push(conditions);
        *input = after_open[close + 1..].trim_start();
    }
    (!lists.is_empty()).then_some(lists)
}

fn valid_state_token(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    let mut scheme_bytes = scheme.bytes();
    if !scheme_bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic())
        || !scheme_bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    {
        return false;
    }

    let bytes = rest.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
            continue;
        }
        if !(byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b':'
                    | b'/'
                    | b'?'
                    | b'['
                    | b']'
                    | b'@'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
            ))
        {
            return false;
        }
        index += 1;
    }
    true
}

fn valid_entity_tag(value: &str) -> bool {
    let opaque = value.strip_prefix("W/").unwrap_or(value);
    opaque.len() >= 2
        && opaque.starts_with('"')
        && opaque.ends_with('"')
        && !opaque[1..opaque.len() - 1]
            .chars()
            .any(|ch| ch == '"' || ch.is_control())
}

fn untagged_if_matches(
    header: &str,
    active_lock_token: Option<&str>,
    etag: Option<&str>,
) -> Option<bool> {
    let IfHeader::Untagged(lists) = parse_if_header(header)? else {
        return None;
    };
    Some(if_lists_match(&lists, active_lock_token, etag))
}

fn if_lists_match(
    lists: &[Vec<IfCondition>],
    active_lock_token: Option<&str>,
    etag: Option<&str>,
) -> bool {
    lists.iter().any(|conditions| {
        let mut has_positive_lock_token = false;
        let conditions_match = conditions.iter().all(|condition| match condition {
            IfCondition::Token { value, negated } => {
                let matches = active_lock_token.is_some_and(|active| value == active);
                if matches && !negated {
                    has_positive_lock_token = true;
                }
                matches != *negated
            }
            IfCondition::EntityTag { value, negated } => {
                let matches = etag.is_some_and(|current| {
                    value.trim_start_matches("W/") == current.trim_start_matches("W/")
                });
                matches != *negated
            }
        });
        conditions_match && active_lock_token.is_none_or(|_| has_positive_lock_token)
    })
}

fn resource_tag_matches(tag: &str, rel: &str, mount_prefix: &str) -> bool {
    if tag.contains('#') {
        return false;
    }
    let Ok(uri) = axum::http::Uri::try_from(tag) else {
        return false;
    };
    if uri.query().is_some() {
        return false;
    }
    let Some(path) = uri.path().strip_prefix('/') else {
        return false;
    };
    let path = percent_decode(path);
    let mounted_prefix = mount_prefix.trim_matches('/');
    let path = if mounted_prefix.is_empty() {
        path.as_str()
    } else if path == mounted_prefix {
        ""
    } else if let Some(rest) = path.strip_prefix(mounted_prefix) {
        let Some(rest) = rest.strip_prefix('/') else {
            return false;
        };
        rest
    } else {
        return false;
    };
    path.trim_end_matches('/') == rel.trim_matches('/')
}

fn if_header_matches_resource(
    header: &str,
    rel: &str,
    mount_prefix: &str,
    active_lock_token: Option<&str>,
    etag: Option<&str>,
) -> Option<bool> {
    match parse_if_header(header)? {
        IfHeader::Untagged(lists) => Some(if_lists_match(&lists, active_lock_token, etag)),
        IfHeader::Tagged(tagged) => {
            let matching_lists: Vec<_> = tagged
                .iter()
                .filter(|(tag, _)| resource_tag_matches(tag, rel, mount_prefix))
                .flat_map(|(_, lists)| lists.iter().cloned())
                .collect();
            Some(if matching_lists.is_empty() {
                active_lock_token.is_none()
            } else {
                if_lists_match(&matching_lists, active_lock_token, etag)
            })
        }
    }
}

fn tagged_if_matches_resource(
    header: &str,
    rel: &str,
    mount_prefix: &str,
    active_lock_token: &str,
    etag: Option<&str>,
) -> Option<bool> {
    let IfHeader::Tagged(tagged) = parse_if_header(header)? else {
        return Some(false);
    };
    let matching_lists: Vec<_> = tagged
        .iter()
        .filter(|(tag, _)| resource_tag_matches(tag, rel, mount_prefix))
        .flat_map(|(_, lists)| lists.iter().cloned())
        .collect();
    Some(
        !matching_lists.is_empty()
            && if_lists_match(&matching_lists, Some(active_lock_token), etag),
    )
}

#[derive(Debug, Clone, Default)]
struct DestinationContext {
    authority: Option<String>,
    scheme: Option<String>,
}

impl DestinationContext {
    fn from_request(uri: &axum::http::Uri, headers: &axum::http::HeaderMap) -> Self {
        let authority = uri.authority().map(ToString::to_string).or_else(|| {
            headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        });
        Self {
            authority,
            scheme: uri.scheme_str().map(str::to_string),
        }
    }
}

fn destination_path_for_request(
    destination: &str,
    mount: &str,
    context: &DestinationContext,
) -> Option<String> {
    if let Ok(uri) = axum::http::Uri::try_from(destination)
        && let Some(authority) = uri.authority()
    {
        let scheme = uri.scheme_str()?;
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return None;
        }
        let request_authority = context.authority.as_deref()?;
        if !same_webdav_authority(
            authority,
            request_authority,
            scheme,
            context.scheme.as_deref(),
        ) {
            return None;
        }
        if context
            .scheme
            .as_deref()
            .is_some_and(|request_scheme| !request_scheme.eq_ignore_ascii_case(scheme))
        {
            return None;
        }
    }
    destination_path(destination, mount)
}

fn same_webdav_authority(
    destination: &axum::http::uri::Authority,
    request: &str,
    destination_scheme: &str,
    request_scheme: Option<&str>,
) -> bool {
    let Ok(request) = axum::http::uri::Authority::try_from(request) else {
        return false;
    };
    if !destination.host().eq_ignore_ascii_case(request.host()) {
        return false;
    }
    let default_port = |scheme: &str| match scheme.to_ascii_lowercase().as_str() {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    };
    let destination_port = destination
        .port_u16()
        .or_else(|| default_port(destination_scheme));
    let request_port = request.port_u16().or_else(|| {
        request_scheme.and_then(default_port).or({
            // Origin-form requests do not carry a scheme. A bare Host can be
            // the default port for either HTTP or HTTPS.
            match destination_port {
                Some(port @ (80 | 443)) => Some(port),
                _ => None,
            }
        })
    });
    match (destination_port, request_port) {
        (Some(destination), Some(request)) => destination == request,
        _ => true,
    }
}

/// LOCK（exclusive write / depth 0 or infinity; omitted Depth defaults to infinity）。
async fn lock_op(
    app: Option<WebdavApplication>,
    user: Option<vfiles_domain::types::User>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_path: String,
    timeout_owned: Option<String>,
    depth_owned: Option<String>,
    lock_body: Vec<u8>,
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
    let depth_infinity = match depth_owned.as_deref().map(str::trim) {
        None => true,
        Some(depth) if depth.eq_ignore_ascii_case("infinity") => true,
        Some("0") => false,
        Some(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap();
        }
    };
    let lockinfo = match parse_lockinfo(&lock_body) {
        Ok(lockinfo) => lockinfo,
        Err(status) => {
            return Response::builder()
                .status(status)
                .body(Body::empty())
                .unwrap();
        }
    };
    let rel = uri_path
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();
    let path = match vfiles_domain::types::NormalizedPath::new(&rel) {
        Ok(path) => path,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap();
        }
    };
    // r15 Timeout（Second-N 解析 ✗ None = Infinite/缺省 = 永久（RFC 缺省语义 ✓））
    let ttl = timeout_owned
        .as_deref()
        .and_then(crate::lock::LockTable::parse_timeout_header);
    let granted_header = match ttl {
        Some(d) => format!("Second-{}", d.as_secs()),
        None => "Infinite".to_string(),
    };
    let owner = lockinfo.owner.unwrap_or_else(|| user.username.to_string());
    match app
        .locks
        .lock(&ns, &rel, &owner, depth_infinity, ttl, lockinfo.scope)
        .await
    {
        Ok(Some(entry)) => {
            let created = if rel.is_empty() {
                // The namespace root is a mapped collection with no ordinary entry row.
                false
            } else {
                match app.entry_repo.find_by_path(&ns, &path).await {
                    Ok(Some(_)) => false,
                    Ok(None) => match app
                        .write
                        .put_file(&ns, &path, Box::new(tokio::io::empty()), &user.id)
                        .await
                    {
                        Ok(created) => created,
                        Err(error) => {
                            if let Err(cleanup_error) =
                                app.locks.unlock(&ns, &rel, &entry.token).await
                            {
                                tracing::error!(%cleanup_error, path = %rel, "LOCK-null 资源创建失败后释放锁失败");
                            }
                            return Response::builder()
                                .status(write_error_status(&error))
                                .body(Body::empty())
                                .unwrap();
                        }
                    },
                    Err(error) => {
                        if let Err(cleanup_error) = app.locks.unlock(&ns, &rel, &entry.token).await
                        {
                            tracing::error!(%cleanup_error, path = %rel, "LOCK-null 资源查询失败后释放锁失败");
                        }
                        tracing::error!(%error, path = %rel, "LOCK 资源查询失败");
                        return internal_error();
                    }
                }
            };
            Response::builder()
                .status(if created {
                    StatusCode::CREATED
                } else {
                    StatusCode::OK
                })
                .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                .header("Lock-Token", format!("<{}>", entry.token))
                .header("Timeout", granted_header.clone())
                .body(Body::from(crate::response::lock_response(
                    &entry.token,
                    &entry.owner,
                    &href_with_mount(&app.mount_prefix, &rel),
                    &granted_header,
                    depth_infinity,
                    entry.scope,
                )))
                .unwrap()
        }
        Ok(None) => {
            if depth_infinity {
                match app.locks.blocked_under_path(&ns, &rel).await {
                    Ok(locks) => {
                        if let Some(conflict) = locks.values().find(|lock| lock.path != rel) {
                            return Response::builder()
                                .status(StatusCode::MULTI_STATUS)
                                .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                                .body(Body::from(crate::response::lock_conflict_response(
                                    &href_with_mount(&app.mount_prefix, &rel),
                                    &href_with_mount(&app.mount_prefix, &conflict.path),
                                )))
                                .unwrap();
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "WebDAV LOCK 冲突路径查询失败");
                        return internal_error();
                    }
                }
            }
            Response::builder()
                .status(StatusCode::LOCKED)
                .body(Body::empty())
                .unwrap()
        }
        Err(error) => {
            tracing::error!(%error, "WebDAV LOCK 存储失败");
            internal_error()
        }
    }
}

struct ParsedLockInfo {
    scope: vfiles_domain::WebdavLockScope,
    owner: Option<String>,
}

/// 解析 RFC 4918 exclusive/shared write 锁请求。
fn parse_lockinfo(body: &[u8]) -> Result<ParsedLockInfo, StatusCode> {
    let xml = std::str::from_utf8(body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let doc = roxmltree::Document::parse(xml).map_err(|_| StatusCode::BAD_REQUEST)?;
    let root = doc.root_element();
    if root.tag_name().namespace() != Some("DAV:") || root.tag_name().name() != "lockinfo" {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut scopes = root.children().filter(|n| {
        n.is_element()
            && n.tag_name().namespace() == Some("DAV:")
            && n.tag_name().name() == "lockscope"
    });
    let scope = scopes.next().ok_or(StatusCode::BAD_REQUEST)?;
    if scopes.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let scope_elements: Vec<_> = scope.children().filter(|node| node.is_element()).collect();
    if scope_elements.len() != 1 || scope_elements[0].tag_name().namespace() != Some("DAV:") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let scope = scope_elements[0];
    let mut locktypes = root.children().filter(|n| {
        n.is_element()
            && n.tag_name().namespace() == Some("DAV:")
            && n.tag_name().name() == "locktype"
    });
    let locktype = locktypes.next().ok_or(StatusCode::BAD_REQUEST)?;
    if locktypes.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let locktype_elements: Vec<_> = locktype
        .children()
        .filter(|node| node.is_element())
        .collect();
    if locktype_elements.len() != 1
        || locktype_elements[0].tag_name().namespace() != Some("DAV:")
        || locktype_elements[0].tag_name().name() != "write"
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let scope = match scope.tag_name().name() {
        "exclusive" => vfiles_domain::WebdavLockScope::Exclusive,
        "shared" => vfiles_domain::WebdavLockScope::Shared,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let owner = root
        .children()
        .find(|node| {
            node.is_element()
                && node.tag_name().namespace() == Some("DAV:")
                && node.tag_name().name() == "owner"
        })
        .map(crate::response::store_xml_element);
    Ok(ParsedLockInfo { scope, owner })
}

/// 空体 LOCK = RFC 4918 §9.10.2 锁刷新，必须携带 If 条件中的现存令牌。
async fn lock_refresh_op(
    app: Option<WebdavApplication>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_path: String,
    if_header: Option<String>,
    timeout_owned: Option<String>,
) -> Response {
    let Some(app) = app else {
        return internal_error();
    };
    let Some(ns) = ns else {
        return internal_error();
    };
    let Some(if_header) = if_header else {
        return Response::builder()
            .status(StatusCode::PRECONDITION_FAILED)
            .body(Body::empty())
            .unwrap();
    };
    let rel = uri_path
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();
    let locks = match app.locks.blocked_all(&ns, &rel).await {
        Ok(locks) if !locks.is_empty() => locks,
        Ok(_) => {
            return Response::builder()
                .status(StatusCode::PRECONDITION_FAILED)
                .body(Body::empty())
                .unwrap();
        }
        Err(error) => {
            tracing::error!(%error, "WebDAV LOCK 刷新读取锁失败");
            return internal_error();
        }
    };
    let mut selected = None;
    let mut invalid_if = false;
    for lock in locks {
        let etag = match vfiles_domain::types::NormalizedPath::new(&lock.path) {
            Ok(path) => match app.entry_repo.find_by_path(&ns, &path).await {
                Ok(entry) => entry
                    .and_then(|entry| entry.current_version_id)
                    .as_ref()
                    .map(derive_etag),
                Err(error) => {
                    tracing::error!(%error, rel, "WebDAV If 条件读取实体标签失败");
                    return internal_error();
                }
            },
            Err(_) => None,
        };
        match if_header_matches_resource(
            &if_header,
            &lock.path,
            &app.mount_prefix,
            Some(&lock.token),
            etag.as_deref(),
        ) {
            Some(true) => selected = Some(lock),
            Some(false) => {}
            None => invalid_if = true,
        }
    }
    let Some(lock) = selected else {
        return Response::builder()
            .status(if invalid_if {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::PRECONDITION_FAILED
            })
            .body(Body::empty())
            .unwrap();
    };
    let lock_path = lock.path.as_str();
    let ttl = timeout_owned
        .as_deref()
        .and_then(crate::lock::LockTable::parse_timeout_header);
    let granted_header = ttl
        .map(|d| format!("Second-{}", d.as_secs()))
        .unwrap_or_else(|| "Infinite".to_string());
    match app.locks.refresh(&ns, lock_path, &lock.token, ttl).await {
        Ok(Some(entry)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
            .header("Timeout", granted_header.clone())
            .body(Body::from(crate::response::lock_response(
                &entry.token,
                &entry.owner,
                &href_with_mount(&app.mount_prefix, lock_path),
                &granted_header,
                entry.depth_infinity,
                entry.scope,
            )))
            .unwrap(),
        Ok(None) => Response::builder()
            .status(StatusCode::PRECONDITION_FAILED)
            .body(Body::empty())
            .unwrap(),
        Err(error) => {
            tracing::error!(%error, "WebDAV LOCK 刷新存储失败");
            internal_error()
        }
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
    let rel = uri_path
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();
    let token = token_raw.as_deref().and_then(parse_lock_token_header);
    let Some(token) = token else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap();
    };
    match app.locks.unlock(&ns, &rel, &token).await {
        Ok(true) => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap(),
        Ok(false) => Response::builder()
            .status(StatusCode::CONFLICT)
            .body(Body::empty())
            .unwrap(),
        Err(error) => {
            tracing::error!(%error, "WebDAV UNLOCK 存储失败");
            internal_error()
        }
    }
}

fn internal_error() -> Response {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::empty())
        .unwrap()
}

fn parse_lock_token_header(value: &str) -> Option<String> {
    let value = value.trim();
    let token = value.strip_prefix('<')?.strip_suffix('>')?;
    valid_state_token(token).then(|| token.to_string())
}

struct GetRequestConditions {
    range: Option<String>,
    if_range: Option<String>,
    if_match: Option<String>,
    if_unmodified_since: Option<String>,
    if_none_match: Option<String>,
    if_modified_since: Option<String>,
    head: bool,
}

#[derive(Debug, Clone, Default)]
struct HttpWriteConditions {
    if_match: Vec<String>,
    if_unmodified_since: Option<String>,
    if_none_match: Option<String>,
}

impl HttpWriteConditions {
    fn from_headers(headers: &axum::http::HeaderMap) -> Result<Option<Self>, ()> {
        let if_match = headers
            .get_all(header::IF_MATCH)
            .iter()
            .map(|value| value.to_str().map(str::to_owned).map_err(|_| ()))
            .collect::<Result<Vec<_>, _>>()?;
        let if_unmodified_values = headers
            .get_all(header::IF_UNMODIFIED_SINCE)
            .iter()
            .collect::<Vec<_>>();
        let if_unmodified_since = match if_unmodified_values.as_slice() {
            [] => None,
            [value] => Some(value.to_str().map(str::to_owned).map_err(|_| ())?),
            _ => Some(String::new()),
        };
        let if_none_match_values = headers
            .get_all(header::IF_NONE_MATCH)
            .iter()
            .map(|value| value.to_str().map(str::to_owned).map_err(|_| ()))
            .collect::<Result<Vec<_>, _>>()?;
        let if_none_match =
            (!if_none_match_values.is_empty()).then(|| if_none_match_values.join(", "));
        let conditions = Self {
            if_match,
            if_unmodified_since,
            if_none_match,
        };
        if conditions.has_any() {
            Ok(Some(conditions))
        } else {
            Ok(None)
        }
    }

    fn has_any(&self) -> bool {
        !self.if_match.is_empty()
            || self.if_unmodified_since.is_some()
            || self.if_none_match.is_some()
    }
}

fn joined_header_values(
    request: &axum::extract::Request,
    name: &axum::http::header::HeaderName,
) -> Option<String> {
    let values: Vec<_> = request.headers().get_all(name).iter().collect();
    (!values.is_empty()).then(|| {
        values
            .into_iter()
            .map(|value| value.to_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", ")
    })
}

fn single_header_value(
    request: &axum::extract::Request,
    name: &axum::http::header::HeaderName,
) -> Option<String> {
    let values: Vec<_> = request.headers().get_all(name).iter().collect();
    match values.as_slice() {
        [] => None,
        [value] => Some(value.to_str().unwrap_or_default().to_string()),
        _ => Some(String::new()),
    }
}

/// PUT（r110'b ✓ 纯拥有参（#46 纪律）✓ 覆盖语义 = 后端版本化（呼应 PROPOSAL ✓））。
/// GET/HEAD（r110'c ✓ 纯拥有参（#46）✓ HEAD = 同头无体 ✓）。
async fn get_op(
    app: Option<WebdavApplication>,
    user: Option<vfiles_domain::types::User>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_owned: String,
    conditions: GetRequestConditions,
) -> Response {
    let GetRequestConditions {
        range: range_owned,
        if_range: if_range_owned,
        if_match,
        if_unmodified_since,
        if_none_match,
        if_modified_since,
        head: is_head,
    } = conditions;
    let Some(app) = app else {
        return internal_error();
    };
    let Some(_user) = user else {
        return www_authenticate();
    };
    let Some(ns) = ns else {
        return internal_error();
    };
    let rel = uri_owned
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();
    let path = match vfiles_domain::types::NormalizedPath::new(&rel) {
        Ok(p) => p,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap();
        }
    };
    // Resolve validators from the same current version used for GET/HEAD.
    let entry = match app
        .entry_repo
        .find_by_path(&ns, &path)
        .await
        .map_err(|error| {
            tracing::error!(path = %rel, %error, "WebDAV GET 读取实体元数据失败");
            error
        }) {
        Ok(entry) => entry,
        Err(_) => return internal_error(),
    };
    let mut cur_etag = None;
    let mut modified_at = entry.as_ref().map(|entry| entry.created_at);
    if let Some(entry) = &entry
        && let Some(version_id) = &entry.current_version_id
    {
        let version = match app.entry_repo.find_version(version_id).await {
            Ok(version) => version,
            Err(error) => {
                tracing::error!(path = %rel, %error, "WebDAV GET 读取当前版本元数据失败");
                return internal_error();
            }
        };
        cur_etag = Some(derive_etag(version_id));
        modified_at = Some(version.created_at);
    }
    let has_representation = entry
        .as_ref()
        .is_some_and(|entry| matches!(entry.entry_type, vfiles_domain::types::EntryKind::File));
    if if_match.as_deref().is_some_and(|condition| {
        !if_match_satisfied(condition, cur_etag.as_deref(), has_representation)
    }) || (if_match.is_none()
        && if_unmodified_since.as_deref().is_some_and(|condition| {
            modified_at.is_some_and(|modified| if_unmodified_since_failed(condition, modified))
        }))
    {
        return conditional_response(
            StatusCode::PRECONDITION_FAILED,
            cur_etag.as_deref(),
            modified_at,
        );
    }
    if if_none_match.as_deref().is_some_and(|condition| {
        if_none_match_satisfied(condition, cur_etag.as_deref(), has_representation)
    }) || (if_none_match.is_none()
        && if_modified_since.as_deref().is_some_and(|condition| {
            modified_at.is_some_and(|modified| if_modified_since_matches(condition, modified))
        }))
    {
        return conditional_response(StatusCode::NOT_MODIFIED, cur_etag.as_deref(), modified_at);
    }
    match app.write.get_stream(&ns, &path).await {
        Ok(Some((mut reader, mime, size))) => {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            // 流式响应（r201 ✓ 大文件不入内存）+ Range 分段（r211 ✓ RFC 7233 ✗
            // 此前忽略 Range = 播放器要 206 给全量 200 = mp4 循环重试真因）
            let mut base = Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header("accept-ranges", "bytes");
            if let Some(et) = cur_etag.as_deref() {
                base = base.header(header::ETAG, et); // r14 GET/206 响应暴露 ETag
            }
            if let Some(modified_at) = modified_at {
                base = base.header(header::LAST_MODIFIED, format_http_date(modified_at));
            }
            let range = if if_range_allows_range(
                if_range_owned.as_deref(),
                cur_etag.as_deref(),
                modified_at,
            ) {
                range_owned
                    .as_deref()
                    .and_then(|rh| match parse_byte_range(rh, size) {
                        ByteRange::Satisfiable(a, b) => Some(Ok((a, b))),
                        ByteRange::Unsatisfiable => Some(Err(())),
                        ByteRange::NotApplicable => None,
                    })
            } else {
                None
            };
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
                        .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
                        .header(header::CONTENT_LENGTH, len.to_string());
                    if is_head {
                        builder.body(Body::empty()).unwrap()
                    } else {
                        let stream = tokio_util::io::ReaderStream::new(reader.take(len));
                        builder.body(Body::from_stream(stream)).unwrap()
                    }
                }
                Some(Err(())) => {
                    let mut builder = Response::builder()
                        .status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .header(header::CONTENT_RANGE, format!("bytes */{size}"));
                    if let Some(etag) = cur_etag.as_deref() {
                        builder = builder.header(header::ETAG, etag);
                    }
                    if let Some(modified_at) = modified_at {
                        builder =
                            builder.header(header::LAST_MODIFIED, format_http_date(modified_at));
                    }
                    builder.body(Body::empty()).unwrap()
                }
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
            user_id: Some(user.id),
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
async fn write_precondition(
    app: &WebdavApplication,
    ns: &vfiles_domain::types::NamespaceId,
    rel: &str,
    if_header: Option<&str>,
) -> Option<StatusCode> {
    let locks = match app.locks.blocked_all(ns, rel).await {
        Ok(entries) => entries,
        Err(error) => {
            tracing::error!(%error, rel, "WebDAV 写锁查询失败");
            return Some(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    if locks.is_empty() {
        if let Some(header) = if_header {
            return match if_header_matches_resource(header, rel, &app.mount_prefix, None, None) {
                Some(true) => None,
                Some(false) => Some(StatusCode::PRECONDITION_FAILED),
                None => Some(StatusCode::BAD_REQUEST),
            };
        }
        return None;
    }
    if let Some(header) = if_header {
        let mut matched = false;
        let mut invalid = false;
        for lock in &locks {
            let condition_rel = lock.path.as_str();
            let etag = if let Ok(path) = vfiles_domain::types::NormalizedPath::new(condition_rel) {
                match app.entry_repo.find_by_path(ns, &path).await {
                    Ok(entry) => entry
                        .and_then(|entry| entry.current_version_id)
                        .as_ref()
                        .map(derive_etag),
                    Err(error) => {
                        tracing::error!(%error, rel, "WebDAV If 条件读取实体标签失败");
                        return Some(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                }
            } else {
                None
            };
            match if_header_matches_resource(
                header,
                condition_rel,
                &app.mount_prefix,
                Some(&lock.token),
                etag.as_deref(),
            ) {
                Some(true) => matched = true,
                Some(false) => {}
                None => invalid = true,
            }
        }
        if matched {
            return None;
        }
        return Some(if invalid {
            StatusCode::BAD_REQUEST
        } else {
            tracing::debug!(rel = %rel, "WebDAV If 条件未匹配，返回 412");
            StatusCode::PRECONDITION_FAILED
        });
    }
    if !locks.is_empty() {
        tracing::debug!(rel = %rel, "WebDAV 写请求缺少锁 token，返回 423");
        return Some(StatusCode::LOCKED);
    }
    None
}

async fn write_subtree_precondition(
    app: &WebdavApplication,
    ns: &vfiles_domain::types::NamespaceId,
    rel: &str,
    if_header: Option<&str>,
) -> Option<StatusCode> {
    if let Some(status) = write_precondition(app, ns, rel, if_header).await {
        return Some(status);
    }
    let locks = match app.locks.blocked_under_path_all(ns, rel).await {
        Ok(locks) => locks,
        Err(error) => {
            tracing::error!(%error, rel, "WebDAV 子树锁查询失败");
            return Some(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let mut locks_by_path = std::collections::HashMap::<String, Vec<_>>::new();
    for lock in locks {
        if lock.path != rel {
            locks_by_path
                .entry(lock.path.clone())
                .or_default()
                .push(lock);
        }
    }
    for (locked_path, path_locks) in locks_by_path {
        let Some(header) = if_header else {
            tracing::debug!(path = %locked_path, "WebDAV 子树资源被锁且缺少 If token，返回 423");
            return Some(StatusCode::LOCKED);
        };
        let etag = match vfiles_domain::types::NormalizedPath::new(&locked_path) {
            Ok(path) => match app.entry_repo.find_by_path(ns, &path).await {
                Ok(entry) => entry
                    .and_then(|entry| entry.current_version_id)
                    .as_ref()
                    .map(derive_etag),
                Err(error) => {
                    tracing::error!(%error, path = %locked_path, "WebDAV 子树 If 条件读取 ETag 失败");
                    return Some(StatusCode::INTERNAL_SERVER_ERROR);
                }
            },
            Err(_) => None,
        };
        let mut matched = false;
        let mut invalid = false;
        for lock in path_locks {
            match tagged_if_matches_resource(
                header,
                &locked_path,
                &app.mount_prefix,
                &lock.token,
                etag.as_deref(),
            ) {
                Some(true) => matched = true,
                Some(false) => {}
                None => invalid = true,
            }
        }
        if !matched {
            return Some(if invalid {
                StatusCode::BAD_REQUEST
            } else {
                tracing::debug!(path = %locked_path, "WebDAV 子树 If 条件未匹配，返回 412");
                StatusCode::PRECONDITION_FAILED
            });
        }
    }
    None
}

async fn put_op(
    app: Option<WebdavApplication>,
    user: Option<vfiles_domain::types::User>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_owned: String,
    if_owned: Option<String>,
    write_condition: Option<vfiles_domain::EntryWriteCondition>,
    put_body: Option<Body>,
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
    let rel = uri_owned
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();
    // r7 锁前置（PUT 此前零检查 ✗ 锁摆设缺口 ×1）
    if let Some(status) = write_precondition(&app, &ns, &rel, if_owned.as_deref()).await {
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
                .unwrap();
        }
    };
    use futures::{StreamExt, TryStreamExt};
    let limit_exceeded = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let exceeded = Arc::clone(&limit_exceeded);
    let body_read_failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let read_failed = Arc::clone(&body_read_failed);
    let max_file_size_bytes = app.max_file_size_bytes;
    let body_stream = body_owned
        .into_data_stream()
        .map_err(std::io::Error::other)
        .scan((0_u64, false), move |(total, stopped), item| {
            if *stopped {
                return std::future::ready(None);
            }
            let item = match item {
                Ok(chunk) => {
                    let next = total.saturating_add(chunk.len() as u64);
                    if next > max_file_size_bytes {
                        *stopped = true;
                        exceeded.store(true, std::sync::atomic::Ordering::Relaxed);
                        return std::future::ready(Some(Err(std::io::Error::new(
                            std::io::ErrorKind::FileTooLarge,
                            "WebDAV PUT exceeds the configured file size limit",
                        ))));
                    }
                    *total = next;
                    Ok(chunk)
                }
                Err(error) => {
                    *stopped = true;
                    read_failed.store(true, std::sync::atomic::Ordering::Relaxed);
                    Err(error)
                }
            };
            std::future::ready(Some(item))
        })
        .boxed();
    let body_reader = tokio_util::io::StreamReader::new(body_stream);
    let result = app
        .write
        .put_file_with_condition(&ns, &path, Box::new(body_reader), &user.id, write_condition)
        .await;
    if limit_exceeded.load(std::sync::atomic::Ordering::Relaxed) {
        return Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body(Body::empty())
            .unwrap();
    }
    if body_read_failed.load(std::sync::atomic::Ordering::Relaxed) {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap();
    }
    match result {
        Ok(created) => Response::builder()
            .status(if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            })
            .body(Body::empty())
            .unwrap(),
        Err(err) => {
            tracing::warn!(path = %rel, error = %err, "WebDAV PUT 失败");
            Response::builder()
                .status(write_error_status(&err))
                .body(Body::empty())
                .unwrap()
        }
    }
}

/// 写操作三型（r108' ✓）。
#[derive(Clone, Copy)]
enum WriteOp {
    Mkcol,
    Delete,
    Move,
}

/// 写操作分派（**纯拥有参** ✓ r105 Send 修复式贯彻（#46：调用侧借用跨 await 同坑二号 ✓））。
#[allow(clippy::too_many_arguments)] // Mirrors the owned request context passed from DAV dispatch.
async fn write_op(
    app: Option<WebdavApplication>,
    user: Option<vfiles_domain::types::User>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    uri_path: String,
    dest_raw: Option<String>,
    if_header: Option<String>,
    op: WriteOp,
    overwrite: bool,
    destination_context: DestinationContext,
    http_conditions: Option<HttpWriteConditions>,
    write_depth_infinity: Option<bool>,
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
    let precondition = if matches!(&op, WriteOp::Delete) {
        write_subtree_precondition(&app, &ns, rel, if_header.as_deref()).await
    } else {
        write_precondition(&app, &ns, rel, if_header.as_deref()).await
    };
    if let Some(status) = precondition {
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
                .unwrap();
        }
    };
    if matches!(op, WriteOp::Delete) && write_depth_infinity == Some(false) {
        match app.entry_repo.find_by_path(&ns, &path).await {
            Ok(Some(entry)) if entry.entry_type == vfiles_domain::types::EntryKind::Directory => {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap();
            }
            Ok(_) => {}
            Err(error) => {
                tracing::error!(%error, path = %rel, "DELETE Depth 校验读取目标失败");
                return internal_error();
            }
        }
    }
    let write_condition = if matches!(op, WriteOp::Delete | WriteOp::Move) {
        match check_http_write_preconditions(&app, &ns, &path, http_conditions.unwrap_or_default())
            .await
        {
            Ok(condition) => condition,
            Err(status) => {
                return Response::builder()
                    .status(status)
                    .body(Body::empty())
                    .unwrap();
            }
        }
    } else {
        None
    };
    if matches!(op, WriteOp::Mkcol) {
        if rel.is_empty() {
            return Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body(Body::empty())
                .unwrap();
        }
        match app.entry_repo.find_by_path(&ns, &path).await {
            Ok(Some(_)) => {
                return Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body(Body::empty())
                    .unwrap();
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(%error, path = %rel, "MKCOL 前检查目标失败");
                return internal_error();
            }
        }
        if let Some((parent_rel, _)) = rel.rsplit_once('/') {
            let parent = match NormalizedPath::new(parent_rel) {
                Ok(parent) => parent,
                Err(_) => return internal_error(),
            };
            match app.entry_repo.find_by_path(&ns, &parent).await {
                Ok(Some(entry))
                    if entry.entry_type == vfiles_domain::types::EntryKind::Directory => {}
                Ok(_) => {
                    return Response::builder()
                        .status(StatusCode::CONFLICT)
                        .body(Body::empty())
                        .unwrap();
                }
                Err(error) => {
                    tracing::error!(%error, parent = %parent_rel, "MKCOL 前检查父目录失败");
                    return internal_error();
                }
            }
        }
    }
    let uid = user.id;
    let overwrite_conflict_is_precondition = matches!(&op, WriteOp::Move) && !overwrite;
    let result = match op {
        WriteOp::Mkcol => app.write.mkcol(&ns, &path, &uid).await,
        WriteOp::Delete => {
            app.write
                .delete_entry_with_condition(&ns, &path, &uid, write_condition)
                .await
        }
        WriteOp::Move => {
            let Some(dest_rel) = dest_raw.as_deref().and_then(|d| {
                destination_path_for_request(d, &app.mount_prefix, &destination_context)
            }) else {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap();
            };
            match NormalizedPath::new(dest_rel.trim_start_matches('/')) {
                Ok(dest) => {
                    app.write
                        .move_entry_with_condition(
                            &ns,
                            &path,
                            &dest,
                            &uid,
                            overwrite,
                            write_condition,
                        )
                        .await
                }
                Err(_) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            }
        }
    };
    match result {
        Ok(()) => {
            if matches!(op, WriteOp::Delete)
                && let Err(error) = app.locks.remove_under_path(&ns, rel).await
            {
                tracing::error!(%error, path = %rel, "WebDAV DELETE 已完成但清理资源锁失败");
                return internal_error();
            }
            if matches!(op, WriteOp::Move) {
                if let Err(error) = app.locks.remove_under_path(&ns, rel).await {
                    tracing::error!(%error, path = %rel, "WebDAV MOVE 已完成但清理源资源锁失败");
                    return internal_error();
                }
                if overwrite
                    && let Some(dest_rel) = dest_raw.as_deref().and_then(|destination| {
                        destination_path_for_request(
                            destination,
                            &app.mount_prefix,
                            &destination_context,
                        )
                    })
                    && let Err(error) = app.locks.remove_under_path(&ns, &dest_rel).await
                {
                    tracing::error!(%error, path = %dest_rel, "WebDAV MOVE 已完成但清理被覆盖目标锁失败");
                    return internal_error();
                }
            }
            Response::builder()
                // r11 分码顺修（RFC：DELETE = 204 ✗ 原三 op 全 201 = 违背顺手修 ✓）
                .status(match op {
                    WriteOp::Delete => StatusCode::NO_CONTENT,
                    WriteOp::Mkcol | WriteOp::Move => StatusCode::CREATED,
                })
                .body(Body::empty())
                .unwrap()
        }
        Err(err) => {
            tracing::warn!(path = %rel, op = "mkcol|delete|move", error = %err, "WebDAV 写操作失败（409）");
            let status = if overwrite_conflict_is_precondition
                && matches!(err, vfiles_domain::DomainError::PathConflict { .. })
            {
                StatusCode::PRECONDITION_FAILED
            } else {
                write_error_status(&err)
            };
            Response::builder()
                .status(status)
                .body(Body::empty())
                .unwrap()
        }
    }
}

async fn check_http_write_preconditions(
    app: &WebdavApplication,
    namespace_id: &vfiles_domain::NamespaceId,
    path: &vfiles_domain::NormalizedPath,
    conditions: HttpWriteConditions,
) -> Result<Option<vfiles_domain::EntryWriteCondition>, StatusCode> {
    if !conditions.has_any() {
        return Ok(None);
    }
    let entry = app
        .entry_repo
        .find_by_path(namespace_id, path)
        .await
        .map_err(|error| {
            tracing::error!(%error, path = %path.as_str(), "WebDAV 写条件校验读取目标失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let exists = entry.is_some();
    let expected_entry_id = entry.as_ref().map(|entry| entry.id);
    let expected_version_id = entry.as_ref().and_then(|entry| entry.current_version_id);
    let etag = expected_version_id.map(|id| derive_etag(&id));
    let mut modified_at = entry.as_ref().map(|entry| entry.created_at);
    if let Some(version_id) = expected_version_id {
        let version = app
            .entry_repo
            .find_version(&version_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, path = %path.as_str(), "WebDAV 写条件校验读取版本失败");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        modified_at = Some(version.created_at);
    }
    let if_match = (!conditions.if_match.is_empty()).then(|| conditions.if_match.join(", "));
    let failed = if_match
        .as_deref()
        .is_some_and(|condition| !if_match_satisfied(condition, etag.as_deref(), exists))
        || (if_match.is_none()
            && conditions
                .if_unmodified_since
                .as_deref()
                .is_some_and(|condition| {
                    modified_at
                        .is_some_and(|modified| if_unmodified_since_failed(condition, modified))
                }))
        || conditions
            .if_none_match
            .as_deref()
            .is_some_and(|condition| if_none_match_satisfied(condition, etag.as_deref(), exists));
    if failed {
        return Err(StatusCode::PRECONDITION_FAILED);
    }
    Ok(Some(vfiles_domain::EntryWriteCondition {
        namespace_id: *namespace_id,
        path: path.clone(),
        expected_entry_id,
        expected_version_id,
    }))
}

fn write_error_status(error: &vfiles_domain::DomainError) -> StatusCode {
    use vfiles_domain::DomainError;

    match error {
        DomainError::Validation { .. }
        | DomainError::UploadPartInvalid
        | DomainError::UploadPartChecksumMismatch
        | DomainError::BlobChecksumMismatch => StatusCode::BAD_REQUEST,
        DomainError::Conflict { .. }
        | DomainError::PathConflict { .. }
        | DomainError::UploadExpired
        | DomainError::UploadConflict => StatusCode::CONFLICT,
        DomainError::PreconditionFailed => StatusCode::PRECONDITION_FAILED,
        DomainError::NotFound { .. }
        | DomainError::EntryNotFound
        | DomainError::VersionNotFound
        | DomainError::SnapshotNotFound => StatusCode::NOT_FOUND,
        DomainError::Unauthorized
        | DomainError::Authentication { .. }
        | DomainError::InvalidCredentials
        | DomainError::SessionExpired
        | DomainError::SessionRevoked => StatusCode::UNAUTHORIZED,
        DomainError::Forbidden | DomainError::UserDisabled => StatusCode::FORBIDDEN,
        DomainError::StorageQuotaExceeded => StatusCode::INSUFFICIENT_STORAGE,
        DomainError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        DomainError::NotImplemented { .. } => StatusCode::NOT_IMPLEMENTED,
        DomainError::SearchIndexNotReady => StatusCode::SERVICE_UNAVAILABLE,
        DomainError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn copy_error_status(error: &vfiles_domain::DomainError, overwrite: bool) -> StatusCode {
    if !overwrite
        && matches!(
            error,
            vfiles_domain::DomainError::Conflict { .. }
                | vfiles_domain::DomainError::PathConflict { .. }
        )
    {
        StatusCode::PRECONDITION_FAILED
    } else {
        write_error_status(error)
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
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&input[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
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

/// ETag 派生（current_version_id → 强 ETag `"hex32"` ✗ r14 零新查询）。
fn derive_etag(v: &vfiles_domain::types::VersionId) -> String {
    format!("\"{}\"", v.to_string().replace('-', ""))
}

fn if_match_satisfied(value: &str, etag: Option<&str>, exists: bool) -> bool {
    let value = value.trim();
    if value == "*" {
        return exists;
    }
    let Some(etag) = etag else {
        return false;
    };
    any_entity_tag_match(value, |candidate| {
        !candidate.starts_with("W/") && candidate == etag
    })
}

fn if_none_match_satisfied(value: &str, etag: Option<&str>, exists: bool) -> bool {
    let value = value.trim();
    if value == "*" {
        return exists;
    }
    let Some(etag) = etag else {
        return false;
    };
    any_entity_tag_match(value, |candidate| {
        candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

fn any_entity_tag_match(value: &str, mut matches: impl FnMut(&str) -> bool) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut matched = false;

    loop {
        while bytes
            .get(index)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            index += 1;
        }
        let start = index;
        if bytes.get(index..index + 2) == Some(b"W/") {
            index += 2;
        }
        if bytes.get(index) != Some(&b'"') {
            return false;
        }
        index += 1;
        while let Some(byte) = bytes.get(index) {
            if *byte == b'"' {
                break;
            }
            if !matches!(*byte, 0x21 | 0x23..=0x7e | 0x80..=0xff) {
                return false;
            }
            index += 1;
        }
        if bytes.get(index) != Some(&b'"') {
            return false;
        }
        index += 1;
        matched |= matches(&value[start..index]);

        while bytes
            .get(index)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            index += 1;
        }
        if index == bytes.len() {
            return matched;
        }
        if bytes.get(index) != Some(&b',') {
            return false;
        }
        index += 1;
        if index == bytes.len() {
            return false;
        }
    }
}

fn modified_system_time(modified_at: time::OffsetDateTime) -> Option<std::time::SystemTime> {
    let seconds = modified_at.unix_timestamp();
    (seconds >= 0)
        .then(|| std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds as u64))
}

fn format_http_date(modified_at: time::OffsetDateTime) -> String {
    let modified = modified_system_time(modified_at).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    httpdate::fmt_http_date(modified)
}

fn if_unmodified_since_failed(value: &str, modified_at: time::OffsetDateTime) -> bool {
    let (Some(modified), Ok(date)) = (
        modified_system_time(modified_at),
        httpdate::parse_http_date(value),
    ) else {
        return false;
    };
    modified > date
}

fn if_modified_since_matches(value: &str, modified_at: time::OffsetDateTime) -> bool {
    let (Some(modified), Ok(date)) = (
        modified_system_time(modified_at),
        httpdate::parse_http_date(value),
    ) else {
        return false;
    };
    modified <= date
}

fn conditional_response(
    status: StatusCode,
    etag: Option<&str>,
    modified_at: Option<time::OffsetDateTime>,
) -> Response {
    let mut builder = Response::builder().status(status);
    if status == StatusCode::NOT_MODIFIED {
        builder = builder.header("cache-control", "private, max-age=0, must-revalidate");
    }
    if let Some(etag) = etag {
        builder = builder.header(header::ETAG, etag);
    }
    if let Some(modified_at) = modified_at {
        builder = builder.header(header::LAST_MODIFIED, format_http_date(modified_at));
    }
    builder.body(Body::empty()).unwrap()
}

/// `Destination` 头 → 相对路径（纯函数 ✓ 单测覆盖）。
///
/// 形 = `http://host/dav/a/b.txt` 或 `/dav/a/b.txt` → `a/b.txt`（去 scheme/host ✓
/// 头必须路径带前导 `/` 否则 400（RFC 4918 §10.3）→ 本式返回 None 由调用方 400 ✓）。
/// href 前缀归一（r-new ✓ mount="" = 独立现行为零变 ✗ "/dav" = 嵌入加前缀；
/// rel 幂等 trim 前导斜杠 → 空 rel = 根（mount+"/"））。
fn href_with_mount(mount: &str, rel: &str) -> String {
    let rel = rel.trim_start_matches('/');
    let path = if mount.is_empty() {
        if rel.is_empty() {
            "/".to_string()
        } else {
            format!("/{rel}")
        }
    } else if rel.is_empty() {
        format!("{}/", mount.trim_end_matches('/'))
    } else {
        format!("{}/{}", mount.trim_end_matches('/'), rel)
    };
    encode_uri_path(&path)
}

/// Percent-encode an absolute URI path while preserving RFC 3986 path characters.
fn encode_uri_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':' | b'@')
            || matches!(
                byte,
                b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            )
        {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

fn destination_path(dest: &str, mount: &str) -> Option<String> {
    let path_part = if dest.contains("://") {
        let uri = axum::http::Uri::try_from(dest).ok()?;
        if uri.query().is_some() {
            return None;
        }
        uri.path().to_string()
    } else {
        if dest.contains(['?', '#']) {
            return None;
        }
        dest.to_string()
    };
    if !path_part.starts_with('/') {
        return None;
    }
    let trimmed = path_part.trim_end_matches('/');
    let rel = percent_decode(trimmed.trim_start_matches('/'));
    // r-new 嵌入模式剥挂载段（standalone mount="" =零变化 ✓）：dest 恰=mount → 根("")
    if !mount.is_empty() {
        let decoded_mount = percent_decode(mount);
        let m = decoded_mount.trim_start_matches('/');
        if rel == m {
            return Some(String::new());
        }
        if let Some(rest) = rel.strip_prefix(&format!("{m}/")) {
            return Some(rest.to_string());
        }
        return None; // 配置了 mount 而 dest 不带 = 外来路径 → 400（防御）
    }
    Some(rel)
}

/// 401 + `WWW-Authenticate: Basic`（RFC 4918 §20.1 ✓）。
fn www_authenticate() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::WWW_AUTHENTICATE, "Basic realm=\"vfiles-webdav\"")
        .body(Body::empty())
        .unwrap()
}

fn login_rate_limited(retry_after_secs: u64) -> Response {
    Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header(header::RETRY_AFTER, retry_after_secs.to_string())
        .body(Body::empty())
        .unwrap()
}

/// WebDAV 能力宣告（无锁 ✓ 子集 ✓）。
// r214 协议声明修正 ✗✗ 此前只声明 4 方法 = 实现了 10 个只报 4 个（客户端靠 Allow
// 判能力 ✗✗）；COPY/PROPPATCH 未实现不声明（声明 = 实力 ✓ 做完再加）
const ALLOW: &str =
    "OPTIONS, PROPFIND, PROPPATCH, GET, HEAD, PUT, DELETE, MKCOL, MOVE, COPY, LOCK, UNLOCK";

fn router(app: WebdavApplication) -> Router {
    use axum::Extension;
    Router::new().fallback(dav).layer(Extension(app))
}

/// PROPFIND（r104 实装 ✓）：Depth 0 = 自身；Depth 1 = 自身 + 直接子条目。
///
/// href 形 = WebDAV 惯例（目录带尾斜杠 ✓）；文件 mtime 取当前版本时间，目录回退到条目创建时间；
/// `deleted_at` 条目假定仓储层已滤（记档 ✓）。
/// PROPFIND（纯拥有参 ✓ `&Request` 跨 await = 非 Send ✗✗ E0277 真因——
/// 同步段提取拥有值是教科书 Send 修复式 ✓ r105 破案记档）。
async fn propfind_owned(
    app: Option<WebdavApplication>,
    ns: Option<vfiles_domain::types::NamespaceId>,
    path: String,
    depth: String,
    body_owned: String,
    user_owned: Option<vfiles_domain::types::User>,
) -> Result<String, StatusCode> {
    let app = app.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let ns = ns.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let depth = depth.trim();
    if depth.eq_ignore_ascii_case("infinity") {
        // r2 合规修正 ✗ RFC 4918 §10.2：拒绝 infinity = 403 + DAV:propfind-finite-depth
        //（原 400 = 合规瑕疵 ✗ P1 项随手落 ✓）
        return Err(StatusCode::FORBIDDEN);
    }
    if depth != "0" && depth != "1" {
        return Err(StatusCode::BAD_REQUEST);
    }
    // 请求体解析（r2 P0 ✗ 非法 = 400（调用方 map_err 下述 NOT_FOUND/500 改由本处 400））
    let mode =
        crate::response::parse_propfind_body(&body_owned).map_err(|_| StatusCode::BAD_REQUEST)?;

    let uri_path = path.trim_end_matches('/');
    let rel = uri_path.trim_start_matches('/').trim_end_matches('/');
    let path = vfiles_domain::types::NormalizedPath::new(if rel.is_empty() { "" } else { rel })
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let wants_last_modified = prop_mode_requests(&mode, "getlastmodified");
    let wants_size_or_type = prop_mode_requests(&mode, "getcontentlength")
        || prop_mode_requests(&mode, "getcontenttype");
    let wants_etag = prop_mode_requests(&mode, "getetag");
    let wants_custom = prop_mode_needs_custom_properties(&mode);
    let wants_lock = prop_mode_requests(&mode, "lockdiscovery");
    let mtime_fmt = |t: time::OffsetDateTime| {
        let timestamp = t.unix_timestamp().max(0) as u64;
        httpdate::fmt_http_date(std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp))
    };
    // r16 owner（r109e 隔离下 ≡ 认证者恒等 = 零查询 ✓ 容错 ""）+ RFC3339 创建时间闭包
    let owner_val = user_owned
        .map(|u| u.username.as_str().to_string())
        .unwrap_or_default();
    let cdate_fmt = |t: time::OffsetDateTime| {
        t.format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    };
    let mut items = Vec::new();
    if rel.is_empty() {
        // 根特判（RFC 4918 ✓ 空命名空间无 root Entry 行 ✗ 合成根响应 ✓）
        items.push(crate::response::PropResponse {
            href: href_with_mount(&app.mount_prefix, "/"),
            displayname: "/".to_string(),
            is_collection: true,
            getlastmodified: if wants_last_modified {
                mtime_fmt(time::OffsetDateTime::now_utc())
            } else {
                String::new()
            },
            getcontentlength: None,
            getcontenttype: None,
            custom: Vec::new(),
            getetag: None,
            creationdate: cdate_fmt(time::OffsetDateTime::now_utc()), // 根 = 合成（lastmod 同式 ✓ 记档）
            owner: owner_val.clone(),
            active_lock: if wants_lock {
                active_lock_prop(&app, &ns, rel).await?
            } else {
                Vec::new()
            },
        });
    } else {
        let (entry, size_bytes, mime_type) = if wants_size_or_type {
            let meta = app
                .entry_repo
                .find_by_path_with_meta(&ns, &path)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let Some(meta) = meta else {
                return Err(StatusCode::NOT_FOUND);
            };
            (meta.entry, meta.size_bytes, meta.mime_type)
        } else {
            let entry = app
                .entry_repo
                .find_by_path(&ns, &path)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let Some(entry) = entry else {
                return Err(StatusCode::NOT_FOUND);
            };
            (entry, None, None)
        };
        // r209 真因修复 ✗✗ 此前硬编码 is_collection: true = **文件被报成目录** →
        // gvfs 把文件当目录反复 PROPFIND、永不 GET = 用户"打不开文件"完整因果链
        // （列表 children 判对、查自身判错）；href 尾斜杠同错（gvfs 探了 png/ 实证）
        let is_dir = matches!(entry.entry_type, vfiles_domain::types::EntryKind::Directory);
        // 按路径一次读取当前版本的 length/type 元数据；PROPFIND 不必打开内容流。
        let (getcontentlength, getcontenttype) = if is_dir {
            (None, None)
        } else {
            (size_bytes, mime_type)
        };
        let custom = if wants_custom {
            app.entry_repo
                .list_entry_properties(&[entry.id])
                .await
                .map_err(|error| {
                    tracing::error!(%error, path = %rel, "WebDAV PROPFIND 属性读取失败");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                .remove(&entry.id)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let getetag = wants_etag
            .then(|| {
                entry
                    .current_version_id
                    .as_ref()
                    .map(|v| format!("\"{}\"", v.to_string().replace('-', "")))
            })
            .flatten();
        let self_href = if is_dir {
            format!("/{rel}/")
        } else {
            format!("/{rel}")
        };
        items.push(crate::response::PropResponse {
            href: href_with_mount(&app.mount_prefix, &self_href),
            displayname: entry.name.clone(),
            is_collection: is_dir,
            getlastmodified: if wants_last_modified {
                mtime_fmt(match entry.current_version_id.as_ref() {
                    Some(version_id) => {
                        app.entry_repo
                            .find_version(version_id)
                            .await
                            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                            .created_at
                    }
                    None => entry.created_at,
                })
            } else {
                String::new()
            },
            getcontentlength,
            getcontenttype,
            custom,
            getetag,
            creationdate: cdate_fmt(entry.created_at),
            owner: owner_val.clone(),
            active_lock: if wants_lock {
                active_lock_prop(&app, &ns, rel).await?
            } else {
                Vec::new()
            },
        });
    }
    if depth == "1" {
        // r4 批量版（N+1 消 ✗✗ 一条 SQL 直取 size/mime ✗ 替换每文件 open）
        let metas = if wants_size_or_type {
            app.entry_repo
                .children_with_meta(&ns, &path)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        } else {
            app.entry_repo
                .find_children(&ns, &path)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .into_iter()
                .map(|entry| vfiles_domain::types::EntryChildMeta {
                    entry,
                    size_bytes: None,
                    mime_type: None,
                    source_mtime: None,
                })
                .collect()
        };
        // r13 自定义属性批量（ids 一次 ✗ r4 批量式复用）
        let child_ids: Vec<vfiles_domain::types::EntryId> =
            metas.iter().map(|m| m.entry.id).collect();
        let child_props = if wants_custom && !child_ids.is_empty() {
            app.entry_repo
                .list_entry_properties(&child_ids)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "WebDAV PROPFIND 子项属性读取失败");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
        } else {
            std::collections::HashMap::new()
        };
        let version_mtimes: std::collections::HashMap<_, _> = if wants_last_modified {
            let version_ids: Vec<_> = metas
                .iter()
                .filter_map(|meta| meta.entry.current_version_id)
                .collect();
            app.entry_repo
                .find_versions(&version_ids)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .into_iter()
                .map(|version| (version.id, version.created_at))
                .collect()
        } else {
            std::collections::HashMap::new()
        };
        let child_paths: Vec<String> = metas
            .iter()
            .map(|meta| format!("{}{}", child_prefix(rel), meta.entry.name))
            .collect();
        let mut child_locks: std::collections::HashMap<_, _> = if wants_lock {
            app.locks
                .blocked_many_all(&ns, &child_paths)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "WebDAV PROPFIND 批量锁查询失败");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                .into_iter()
                .map(|(path, locks)| {
                    (
                        path,
                        locks
                            .into_iter()
                            .map(|lock| active_lock_value(lock, &app.mount_prefix))
                            .collect(),
                    )
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        };
        for meta in metas {
            let child = meta.entry;
            let child_rel = format!("{}{}", child_prefix(rel), child.name);
            let is_dir = matches!(child.entry_type, vfiles_domain::types::EntryKind::Directory);
            let (getcontentlength, getcontenttype) = if is_dir {
                (None, None)
            } else {
                (meta.size_bytes, meta.mime_type)
            };
            items.push(crate::response::PropResponse {
                href: href_with_mount(
                    &app.mount_prefix,
                    &entry_href(&child_prefix(rel), &child.name, is_dir),
                ),
                displayname: child.name,
                is_collection: is_dir,
                getlastmodified: if wants_last_modified {
                    mtime_fmt(
                        child
                            .current_version_id
                            .as_ref()
                            .and_then(|version_id| version_mtimes.get(version_id).copied())
                            .unwrap_or(child.created_at),
                    )
                } else {
                    String::new()
                },
                getcontentlength,
                getcontenttype,
                custom: child_props.get(&child.id).cloned().unwrap_or_default(),
                getetag: wants_etag
                    .then(|| {
                        child
                            .current_version_id
                            .as_ref()
                            .map(|v| format!("\"{}\"", v.to_string().replace('-', "")))
                    })
                    .flatten(),
                creationdate: cdate_fmt(child.created_at),
                owner: owner_val.clone(),
                active_lock: child_locks.remove(&child_rel).unwrap_or_default(),
            });
        }
    }
    Ok(crate::response::multistatus(&items, &mode))
}

async fn active_lock_prop(
    app: &WebdavApplication,
    ns: &vfiles_domain::types::NamespaceId,
    rel: &str,
) -> Result<Vec<crate::response::ActiveLock>, StatusCode> {
    let locks = app.locks.blocked_all(ns, rel).await.map_err(|error| {
        tracing::error!(%error, rel, "WebDAV PROPFIND 锁查询失败");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(locks
        .into_iter()
        .map(|lock| active_lock_value(lock, &app.mount_prefix))
        .collect())
}

fn active_lock_value(
    lock: crate::lock::LockEntry,
    mount_prefix: &str,
) -> crate::response::ActiveLock {
    let timeout = match lock.expires_at {
        Some(expiry) => {
            let remaining_ms = expiry.saturating_sub(crate::lock::LockTable::now()).max(1);
            let remaining_seconds = remaining_ms.saturating_add(999) / 1_000;
            format!("Second-{remaining_seconds}")
        }
        None => "Infinite".to_string(),
    };
    crate::response::ActiveLock {
        token: lock.token,
        owner: lock.owner,
        timeout,
        depth_infinity: lock.depth_infinity,
        scope: lock.scope,
        root_href: href_with_mount(mount_prefix, &lock.path),
    }
}

fn prop_mode_requests(mode: &crate::response::PropMode, local_name: &str) -> bool {
    match mode {
        crate::response::PropMode::All | crate::response::PropMode::AllInclude(_) => true,
        crate::response::PropMode::PropName => false,
        crate::response::PropMode::Names(names) => names
            .iter()
            .any(|name| crate::response::is_dav_property(name, local_name)),
    }
}

fn prop_mode_needs_custom_properties(mode: &crate::response::PropMode) -> bool {
    const SUPPORTED: [&str; 10] = [
        "displayname",
        "resourcetype",
        "getlastmodified",
        "getcontentlength",
        "getcontenttype",
        "getetag",
        "creationdate",
        "owner",
        "supportedlock",
        "lockdiscovery",
    ];
    match mode {
        crate::response::PropMode::All
        | crate::response::PropMode::AllInclude(_)
        | crate::response::PropMode::PropName => true,
        crate::response::PropMode::Names(names) => names.iter().any(|name| {
            crate::response::is_dav_property(name, "displayname")
                || !SUPPORTED
                    .iter()
                    .any(|local_name| crate::response::is_dav_property(name, local_name))
        }),
    }
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

fn if_range_allows_range(
    if_range: Option<&str>,
    etag: Option<&str>,
    modified_at: Option<time::OffsetDateTime>,
) -> bool {
    let Some(if_range) = if_range else {
        return true;
    };
    let if_range = if_range.trim();
    if !if_range.starts_with("W/") && etag.is_some_and(|current| if_range == current) {
        return true;
    }
    let (Some(modified), Ok(date)) = (
        modified_at.and_then(modified_system_time),
        httpdate::parse_http_date(if_range),
    ) else {
        return false;
    };
    modified <= date
}

fn a_is_empty_n(suffix: &str) -> Option<usize> {
    suffix.parse::<usize>().ok()
}

/// 认证成功首行标记（r210 降噪：首条 info、其后 debug ✗ 连接可见且不刷屏）
static AUTH_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 方法分派（PROPFIND 等非标方法经 `any` 到达 ✓）。
#[axum::debug_handler]
async fn dav(req: axum::extract::Request) -> Response {
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
    // Take ownership before authentication awaits, but do not poll the upload
    // body until the authorized write path consumes it.
    let put_body = if req.method() == axum::http::Method::PUT {
        Some(std::mem::take(req.body_mut()))
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
        let peer_ip = req
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|axum::extract::ConnectInfo(peer)| peer.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let login_key = format!("ip:{peer_ip}|login:{}", u.trim().to_ascii_lowercase());
        if let Some(block) = app_ref
            .login_attempt_limiter
            .check(&app_ref.login_rate_limit, &login_key)
        {
            app_ref.ingest_stats.record_login_failure();
            tracing::warn!(username = %u, retry_after_secs = block.retry_after_secs, "WebDAV 认证尝试被限流拒绝");
            return login_rate_limited(block.retry_after_secs);
        }
        let log_name = u.clone(); // 日志副本（u move 进 verify ✗ 先留名）
        let Some(user) = (app_ref.verify)(u, pw).await else {
            app_ref
                .login_attempt_limiter
                .record_failure(&app_ref.login_rate_limit, &login_key);
            app_ref.ingest_stats.record_login_failure();
            // 认证失败 = warn（用户排查关键行 ✗ 服务端日志记 username 不回客户端 ✓）
            tracing::warn!(username = %log_name, "WebDAV 认证失败（401）——检查用户名/密码，或账号是否被禁用");
            return www_authenticate();
        };
        app_ref.login_attempt_limiter.clear(&login_key);
        // 首次成功 = info（连接可见 ✓）后续 debug（不每请求刷 ✗✗ r210 用户刷屏抱怨）
        if !AUTH_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::info!(username = %user.username.as_str(), "WebDAV 认证成功（本次连接后归 debug）");
        } else {
            tracing::debug!(username = %user.username.as_str(), "WebDAV 认证成功");
        }
        // per-user ns 动态映射（r109e ✓ ensure_default_for_owner ✓ 多用户隔离）
        let ns = match app_ref.namespaces.ensure_default_for_owner(&user.id).await {
            Ok(ns) => ns,
            Err(_) => return internal_error(),
        };
        req.extensions_mut().insert(user);
        req.extensions_mut().insert(ns);
    }
    if req.headers().get_all("if").iter().count() > 1 {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap();
    }
    match req.method().clone() {
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
            let if_range_values = req.headers().get_all("if-range").iter().collect::<Vec<_>>();
            let if_range_owned = match if_range_values.as_slice() {
                [] => None,
                [value] => Some(value.to_str().unwrap_or_default().to_string()),
                _ => Some(String::new()),
            };
            let if_match_owned = joined_header_values(&req, &header::IF_MATCH);
            let if_none_match_owned = joined_header_values(&req, &header::IF_NONE_MATCH);
            let if_unmodified_since_owned = single_header_value(&req, &header::IF_UNMODIFIED_SINCE);
            let if_modified_since_owned = single_header_value(&req, &header::IF_MODIFIED_SINCE);
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req
                .extensions()
                .get::<vfiles_domain::types::User>()
                .cloned();
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let uri_owned = percent_decode(req.uri().path());
            get_op(
                app_owned,
                user_owned,
                ns_owned,
                uri_owned,
                GetRequestConditions {
                    range: range_owned,
                    if_range: if_range_owned,
                    if_match: if_match_owned,
                    if_unmodified_since: if_unmodified_since_owned,
                    if_none_match: if_none_match_owned,
                    if_modified_since: if_modified_since_owned,
                    head: is_head,
                },
            )
            .await
        }
        ref m if m.as_str() == "PROPFIND" => {
            // 同步提取拥有值（&Request 跨 await = 非 Send ✗✗ E0277 真因 ✓ r105 破案）
            let path_owned = percent_decode(req.uri().path());
            let mut depth_values = req.headers().get_all("depth").iter();
            let depth_owned = match (depth_values.next(), depth_values.next()) {
                (None, _) => "infinity".to_string(), // RFC 4918 §10.2 default
                (Some(value), None) => value.to_str().unwrap_or_default().to_string(),
                (Some(_), Some(_)) => String::new(),
            };
            let app = req.extensions().get::<WebdavApplication>().cloned();
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let body_owned = {
                // #46 同步 take 转 owned（await 前结束借 ✓）
                let taken = std::mem::take(req.body_mut());
                match axum::body::to_bytes(taken, MAX_DAV_XML_BODY_BYTES).await {
                    Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
                        Ok(body) => body,
                        Err(_) => {
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap();
                        }
                    },
                    Err(_) => {
                        return Response::builder()
                            .status(StatusCode::PAYLOAD_TOO_LARGE)
                            .body(Body::empty())
                            .unwrap();
                    }
                }
            };
            match propfind_owned(
                app,
                ns_owned,
                path_owned,
                depth_owned,
                body_owned,
                req.extensions()
                    .get::<vfiles_domain::types::User>()
                    .cloned(),
            )
            .await
            {
                Ok(xml) => Response::builder()
                    .status(StatusCode::MULTI_STATUS)
                    .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                    .body(Body::from(xml))
                    .unwrap(),
                Err(StatusCode::FORBIDDEN) => Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                    .body(Body::from(
                        r#"<?xml version="1.0" encoding="utf-8"?><D:error xmlns:D="DAV:"><D:propfind-finite-depth/></D:error>"#,
                    ))
                    .unwrap(),
                Err(status) => Response::builder()
                    .status(status)
                    .body(Body::empty())
                    .unwrap(),
            }
        }
        // LOCK/UNLOCK（r109a ✓ 商业级核心件 = Windows 映射依赖）。
        // 纯拥有参（#46 三号实录强化 ✗✗ 借用不跨 await = 编码模板纪律）。
        ref m if m.as_str() == "LOCK" || m.as_str() == "UNLOCK" => {
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req
                .extensions()
                .get::<vfiles_domain::types::User>()
                .cloned();
            let uri_owned = percent_decode(req.uri().path());
            let token_owned = req
                .headers()
                .get("lock-token")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let if_header_owned = req
                .headers()
                .get("if")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let timeout_owned = req
                .headers()
                .get("timeout")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let mut depth_values = req.headers().get_all("depth").iter();
            let depth_owned = match (depth_values.next(), depth_values.next()) {
                (None, None) => None,
                (Some(value), None) => Some(value.to_str().map(str::to_string).unwrap_or_default()),
                _ => Some(String::new()),
            };
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let body = std::mem::replace(req.body_mut(), Body::empty());
            let body_bytes = match axum::body::to_bytes(body, 64 * 1024).await {
                Ok(bytes) => bytes.to_vec(),
                Err(_) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            let resp = if m.as_str() == "LOCK" && body_bytes.is_empty() {
                lock_refresh_op(
                    app_owned,
                    ns_owned,
                    uri_owned,
                    if_header_owned,
                    timeout_owned,
                )
                .await
            } else if m.as_str() == "LOCK" {
                lock_op(
                    app_owned,
                    user_owned,
                    ns_owned,
                    uri_owned,
                    timeout_owned,
                    depth_owned,
                    body_bytes,
                )
                .await
            } else {
                unlock_op(app_owned, ns_owned, uri_owned, token_owned).await
            };
            if resp.status().is_success() {
                if let (Some(app), Some(u)) = (
                    req.extensions().get::<WebdavApplication>(),
                    req.extensions().get::<vfiles_domain::types::User>(),
                ) {
                    let action = if m.as_str() == "LOCK" {
                        "webdav.lock"
                    } else {
                        "webdav.unlock"
                    };
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
            } else if resp.status() == StatusCode::CONFLICT || resp.status().as_u16() >= 500 {
                // r17 失败面审计表记（409/5xx → Failure ✗ 423/412 拒绝 = warn 覆盖范围注 ✓）
                if let (Some(app), Some(u)) = (
                    req.extensions().get::<WebdavApplication>(),
                    req.extensions().get::<vfiles_domain::types::User>(),
                ) {
                    audit_write(
                        app,
                        if m.as_str() == "LOCK" {
                            "webdav.lock"
                        } else {
                            "webdav.unlock"
                        },
                        percent_decode(req.uri().path()),
                        u,
                        req.headers()
                            .get("user-agent")
                            .and_then(|v| v.to_str().ok()),
                        vfiles_domain::types::AuditResult::Failure,
                    );
                }
            }
            resp
        }
        // 写面由 MKCOL/DELETE/MOVE/COPY 与流式 PUT 分支处理。
        // 同步提取拥有值（借用不跨 await ✓ #46）。
        ref m if m.as_str() == "PROPPATCH" => {
            // PROPPATCH parses DAV property names and stores changes transactionally.
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req
                .extensions()
                .get::<vfiles_domain::types::User>()
                .cloned();
            let uri_owned = percent_decode(req.uri().path());
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let body_owned = {
                let taken = std::mem::take(req.body_mut());
                match axum::body::to_bytes(taken, MAX_DAV_XML_BODY_BYTES).await {
                    Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
                        Ok(body) => body,
                        Err(_) => {
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap();
                        }
                    },
                    Err(_) => {
                        return Response::builder()
                            .status(StatusCode::PAYLOAD_TOO_LARGE)
                            .body(Body::empty())
                            .unwrap();
                    }
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
                write_precondition(&app_ref, &ns, path.as_str(), if_owned.as_deref()).await
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
            let mut results: Vec<(crate::response::PropOp, crate::response::PropPatchStatus)> =
                Vec::with_capacity(ops.len());
            if let Some(failed_index) = ops.iter().position(|op| match op {
                crate::response::PropOp::Set { name, .. } => {
                    crate::response::is_predefined_readonly(name)
                }
                crate::response::PropOp::Remove { name } => {
                    crate::response::is_predefined_readonly(name)
                }
            }) {
                results.extend(ops.into_iter().enumerate().map(|(i, op)| {
                    let status = if i == failed_index {
                        crate::response::PropPatchStatus::Forbidden
                    } else {
                        crate::response::PropPatchStatus::FailedDependency
                    };
                    (op, status)
                }));
            } else {
                let entry = match app_ref.entry_repo.find_by_path(&ns, &path).await {
                    Ok(Some(entry)) => entry,
                    Ok(None) => {
                        return Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::empty())
                            .unwrap();
                    }
                    Err(error) => {
                        tracing::error!(%error, path = path.as_str(), "WebDAV PROPPATCH 资源查询失败");
                        return internal_error();
                    }
                };
                let changes: Vec<vfiles_domain::EntryPropertyChange> = ops
                    .iter()
                    .map(|op| match op {
                        crate::response::PropOp::Set { name, value } => {
                            vfiles_domain::EntryPropertyChange::Set {
                                name: name.clone(),
                                value: value.clone(),
                            }
                        }
                        crate::response::PropOp::Remove { name } => {
                            vfiles_domain::EntryPropertyChange::Remove { name: name.clone() }
                        }
                    })
                    .collect();
                if let Err(error) = app_ref
                    .entry_repo
                    .apply_entry_property_changes(&entry.id, &changes)
                    .await
                {
                    tracing::error!(%error, path = path.as_str(), "WebDAV PROPPATCH 事务失败");
                    results.extend(
                        ops.into_iter()
                            .map(|op| (op, crate::response::PropPatchStatus::InternalServerError)),
                    );
                } else {
                    results.extend(
                        ops.into_iter()
                            .map(|op| (op, crate::response::PropPatchStatus::Ok)),
                    );
                }
            }
            if let Some(cb) = &app_ref.audit {
                cb(vfiles_domain::types::NewAuditLog {
                    user_id: Some(user.id),
                    username: user.username.as_str().to_string(),
                    action: "webdav.proppatch".to_string(),
                    result: if results
                        .iter()
                        .all(|(_, status)| *status == crate::response::PropPatchStatus::Ok)
                    {
                        vfiles_domain::types::AuditResult::Success
                    } else {
                        vfiles_domain::types::AuditResult::Failure
                    },
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
            let xml = crate::response::proppatch_multistatus(
                &href_with_mount(&app_ref.mount_prefix, path.as_str()),
                &results,
            );
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
            let user_owned = req
                .extensions()
                .get::<vfiles_domain::types::User>()
                .cloned();
            let uri_owned = percent_decode(req.uri().path());
            // r-new：先取原文，解析延后到 app_ref 解包后（mount 需 app ✗ 作用域序修）
            let dest_raw = req
                .headers()
                .get("destination")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let (app_ref, user, ns) = match (app_owned, user_owned, ns_owned) {
                (Some(a), Some(u), Some(n)) => (a, u, n),
                _ => return internal_error(),
            };
            let destination_context = DestinationContext::from_request(req.uri(), req.headers());
            let dest_hdr = dest_raw.and_then(|d| {
                destination_path_for_request(&d, &app_ref.mount_prefix, &destination_context)
            });
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
            // RFC 4918 §10.6: only T/F are valid, with T as the default.
            let overwrite = match parse_overwrite_header(req.headers()) {
                Some(overwrite) => overwrite,
                None => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            let mut depth_values = req.headers().get_all("depth").iter();
            let depth_infinity = match (depth_values.next(), depth_values.next()) {
                (None, None) => true,
                (Some(value), None)
                    if value
                        .to_str()
                        .is_ok_and(|value| value.trim().eq_ignore_ascii_case("infinity")) =>
                {
                    true
                }
                (Some(value), None) if value.to_str().is_ok_and(|value| value.trim() == "0") => {
                    false
                }
                _ => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            // 源路径裁前导斜杠（PROPFIND 同式 ✗ 真因：new 不收前导 / ✗ 诊断日志定案 ✓）
            let src_rel = uri_owned
                .trim_start_matches('/')
                .trim_end_matches('/')
                .to_string();
            // COPY reads the source without changing it; only the destination subtree
            // participates in write-lock preconditions.
            if let Some(status) = write_subtree_precondition(
                &app_ref,
                &ns,
                dest_hdr.trim_matches('/'),
                if_owned.as_deref(),
            )
            .await
            {
                return Response::builder()
                    .status(status)
                    .body(Body::empty())
                    .unwrap();
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
            let http_conditions = match HttpWriteConditions::from_headers(req.headers()) {
                Ok(conditions) => conditions.unwrap_or_default(),
                Err(()) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap();
                }
            };
            let copy_condition =
                match check_http_write_preconditions(&app_ref, &ns, &path, http_conditions).await {
                    Ok(condition) => condition,
                    Err(status) => {
                        return Response::builder()
                            .status(status)
                            .body(Body::empty())
                            .unwrap();
                    }
                };
            let user_id = user.id;
            let username = user.username.as_str().to_string();
            let dst_existed = match app_ref.entry_repo.find_by_path(&ns, &dest_path).await {
                Ok(entry) => entry.is_some(),
                Err(error) => {
                    tracing::error!(%error, target = %dest_hdr, "COPY 目标状态查询失败");
                    return internal_error();
                }
            };
            if let Some((parent_rel, _)) = dest_hdr.rsplit_once('/') {
                let parent = match vfiles_domain::types::NormalizedPath::new(parent_rel) {
                    Ok(parent) => parent,
                    Err(_) => {
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::empty())
                            .unwrap();
                    }
                };
                match app_ref.entry_repo.find_by_path(&ns, &parent).await {
                    Ok(Some(entry))
                        if entry.entry_type == vfiles_domain::types::EntryKind::Directory => {}
                    Ok(_) => {
                        return Response::builder()
                            .status(StatusCode::CONFLICT)
                            .body(Body::empty())
                            .unwrap();
                    }
                    Err(error) => {
                        tracing::error!(%error, parent = %parent_rel, "COPY 目标父集合查询失败");
                        return internal_error();
                    }
                }
            }
            // Overwrite: F + 目标存在 → 412（RFC §9.3.3 ✗ r5 曾全 409 = 违背修正）
            if dst_existed && !overwrite {
                return Response::builder()
                    .status(StatusCode::PRECONDITION_FAILED)
                    .body(Body::empty())
                    .unwrap();
            }
            match app_ref
                .write
                .copy_entry_with_condition(
                    &ns,
                    &path,
                    &dest_path,
                    &user.id,
                    crate::write::WebdavCopyOptions {
                        overwrite,
                        depth_infinity,
                        condition: copy_condition,
                    },
                )
                .await
            {
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
                Err(err) => {
                    tracing::warn!(target = %dest_hdr, error = %err, "WebDAV COPY 失败");
                    if let (Some(app), Some(u)) = (
                        req.extensions().get::<WebdavApplication>(),
                        req.extensions().get::<vfiles_domain::types::User>(),
                    ) {
                        audit_write(
                            app,
                            "webdav.copy",
                            format!("{} -> {}", path.as_str(), dest_hdr),
                            u,
                            req.headers()
                                .get("user-agent")
                                .and_then(|v| v.to_str().ok()),
                            vfiles_domain::types::AuditResult::Failure,
                        );
                    }
                    Response::builder()
                        .status(copy_error_status(&err, overwrite))
                        .body(Body::empty())
                        .unwrap()
                }
            }
        }
        ref m if m.as_str() == "MKCOL" || m.as_str() == "DELETE" || m.as_str() == "MOVE" => {
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req
                .extensions()
                .get::<vfiles_domain::types::User>()
                .cloned();
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
            let write_depth_infinity = if matches!(op, WriteOp::Delete | WriteOp::Move) {
                match parse_write_depth(req.headers()) {
                    Ok(depth) => depth,
                    Err(()) => {
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::empty())
                            .unwrap();
                    }
                }
            } else {
                None
            };
            if matches!(op, WriteOp::Move) && write_depth_infinity == Some(false) {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap();
            }
            let mkcol_has_body = if matches!(op, WriteOp::Mkcol) {
                request_body_is_nonempty(req.body_mut()).await
            } else {
                false
            };
            if matches!(op, WriteOp::Mkcol)
                && (mkcol_has_body
                    || req
                        .headers()
                        .contains_key(axum::http::header::TRANSFER_ENCODING))
            {
                return Response::builder()
                    .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
                    .body(Body::empty())
                    .unwrap();
            }
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
                let http_conditions = if matches!(op, WriteOp::Delete | WriteOp::Move) {
                    match HttpWriteConditions::from_headers(req.headers()) {
                        Ok(conditions) => conditions,
                        Err(()) => {
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap();
                        }
                    }
                } else {
                    None
                };
                // r11 MOVE Overwrite（臂层式 ✗ 零签名变 ✓ extensions 重取 = 不碰已 move 变量）
                let mut move_overwrite_204 = false;
                let move_overwrite = if matches!(op, WriteOp::Move) {
                    match parse_overwrite_header(req.headers()) {
                        Some(overwrite) => overwrite,
                        None => {
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap();
                        }
                    }
                } else {
                    true
                };
                if matches!(op, WriteOp::Move)
                    && let (Some(app), Some(_u), Some(ns_ext)) = (
                        req.extensions().get::<WebdavApplication>(),
                        req.extensions().get::<vfiles_domain::types::User>(),
                        req.extensions().get::<vfiles_domain::types::NamespaceId>(),
                    )
                    && let Some(dest_rel) = dest_owned.as_deref().and_then(|d| {
                        destination_path_for_request(
                            d,
                            &app.mount_prefix,
                            &DestinationContext::from_request(req.uri(), req.headers()),
                        )
                    })
                {
                    let source_rel = percent_decode(req.uri().path())
                        .trim_start_matches('/')
                        .trim_end_matches('/')
                        .to_string();
                    if let Some(status) =
                        write_subtree_precondition(app, ns_ext, &source_rel, if_owned.as_deref())
                            .await
                    {
                        return Response::builder()
                            .status(status)
                            .body(Body::empty())
                            .unwrap();
                    }
                    let source = match vfiles_domain::types::NormalizedPath::new(&source_rel) {
                        Ok(source) => source,
                        Err(_) => {
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap();
                        }
                    };
                    if dest_rel == source_rel || dest_rel.starts_with(&format!("{source_rel}/")) {
                        return Response::builder()
                            .status(StatusCode::CONFLICT)
                            .body(Body::empty())
                            .unwrap();
                    }
                    match app.entry_repo.find_by_path(ns_ext, &source).await {
                        Ok(Some(entry))
                            if entry.entry_type == vfiles_domain::types::EntryKind::Directory
                                && write_depth_infinity == Some(false) =>
                        {
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap();
                        }
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            return Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(Body::empty())
                                .unwrap();
                        }
                        Err(error) => {
                            tracing::error!(%error, source = %source_rel, "MOVE 覆盖前读取源失败");
                            return internal_error();
                        }
                    }
                    if let Some(status) =
                        write_subtree_precondition(app, ns_ext, &dest_rel, if_owned.as_deref())
                            .await
                    {
                        return Response::builder()
                            .status(status)
                            .body(Body::empty())
                            .unwrap();
                    }
                    // r12 dest 即 target（WebDAV 完整目标路径 ✗ r11 曾 join 目录
                    // = 违 RFC 二义 → 服务参数化后臂层同步简化 ✓ 同名目录覆盖打通）
                    if let Ok(target) = vfiles_domain::types::NormalizedPath::new(&dest_rel) {
                        let target_exists = match app.entry_repo.find_by_path(ns_ext, &target).await
                        {
                            Ok(entry) => entry.is_some(),
                            Err(error) => {
                                tracing::error!(%error, target = %dest_rel, "MOVE 覆盖前读取目标失败");
                                return internal_error();
                            }
                        };
                        if target_exists {
                            if !move_overwrite {
                                // Overwrite: F + 目标存在 → 412（同 COPY r10 语义）
                                return Response::builder()
                                    .status(StatusCode::PRECONDITION_FAILED)
                                    .body(Body::empty())
                                    .unwrap();
                            }
                            move_overwrite_204 = true;
                        }
                    }
                }
                let resp = write_op(
                    app_owned,
                    user_owned,
                    ns_owned,
                    uri_owned,
                    dest_owned,
                    if_owned,
                    op,
                    move_overwrite,
                    DestinationContext::from_request(req.uri(), req.headers()),
                    http_conditions,
                    write_depth_infinity,
                )
                .await;
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
                } else if resp.status() == StatusCode::CONFLICT || resp.status().as_u16() >= 500 {
                    // r17 失败面审计表记（409/5xx → Failure ✗ 423/412 拒绝 = warn 覆盖范围注 ✓）
                    if let (Some(app), Some(u)) = (
                        req.extensions().get::<WebdavApplication>(),
                        req.extensions().get::<vfiles_domain::types::User>(),
                    ) {
                        audit_write(
                            app,
                            audit_action,
                            percent_decode(req.uri().path()),
                            u,
                            req.headers()
                                .get("user-agent")
                                .and_then(|v| v.to_str().ok()),
                            vfiles_domain::types::AuditResult::Failure,
                        );
                    }
                }
                resp
            }
        }
        // PUT（r110'b ✓ 商业级写面终件 = 流式直传）。
        // PUT（r110'b ✓ 商业级写面终件 = 流式直传）。
        // 纯拥有参（#46 四号 ✗✗✗ 调用侧同步提取）。
        ref m if m.as_str() == "PUT" => {
            let app_owned = req.extensions().get::<WebdavApplication>().cloned();
            let user_owned = req
                .extensions()
                .get::<vfiles_domain::types::User>()
                .cloned();
            let ns_owned = req
                .extensions()
                .get::<vfiles_domain::types::NamespaceId>()
                .cloned();
            let uri_owned = percent_decode(req.uri().path());
            let mut write_condition = None;
            {
                let ua = req
                    .headers()
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                // If-Match uses strong comparison. Combine repeated field lines as an entity-tag
                // list and preserve resource existence independently of whether it has an ETag.
                let if_match_values = req
                    .headers()
                    .get_all(header::IF_MATCH)
                    .iter()
                    .map(|value| value.to_str().map(str::to_owned))
                    .collect::<Result<Vec<_>, _>>();
                let if_match_values = match if_match_values {
                    Ok(values) => values,
                    Err(error) => {
                        tracing::warn!(%error, "WebDAV PUT received an invalid If-Match header");
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::empty())
                            .unwrap();
                    }
                };
                let if_unmodified_since = single_header_value(&req, &header::IF_UNMODIFIED_SINCE);
                let if_none_match = joined_header_values(&req, &header::IF_NONE_MATCH);
                if !if_match_values.is_empty()
                    || if_unmodified_since.is_some()
                    || if_none_match.is_some()
                {
                    let put_rel = uri_owned.trim_start_matches('/').trim_end_matches('/');
                    let put_path = match vfiles_domain::types::NormalizedPath::new(put_rel) {
                        Ok(path) => path,
                        Err(_) => {
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap();
                        }
                    };
                    let (exists, cur, modified_at) = match (app_owned.as_ref(), ns_owned.as_ref()) {
                        (Some(app), Some(ns_e)) => {
                            match app.entry_repo.find_by_path(ns_e, &put_path).await {
                                Ok(entry) => {
                                    let exists = entry.is_some();
                                    write_condition = Some(vfiles_domain::EntryWriteCondition {
                                        namespace_id: *ns_e,
                                        path: put_path.clone(),
                                        expected_entry_id: entry.as_ref().map(|entry| entry.id),
                                        expected_version_id: entry
                                            .as_ref()
                                            .and_then(|entry| entry.current_version_id),
                                    });
                                    let mut cur = None;
                                    let mut modified_at =
                                        entry.as_ref().map(|entry| entry.created_at);
                                    if let Some(version_id) = entry
                                        .as_ref()
                                        .and_then(|entry| entry.current_version_id.as_ref())
                                    {
                                        let version = match app
                                            .entry_repo
                                            .find_version(version_id)
                                            .await
                                        {
                                            Ok(version) => version,
                                            Err(error) => {
                                                tracing::error!(%error, path = %put_rel, "WebDAV PUT conditional validator lookup failed");
                                                return internal_error();
                                            }
                                        };
                                        cur = Some(derive_etag(version_id));
                                        modified_at = Some(version.created_at);
                                    }
                                    (exists, cur, modified_at)
                                }
                                Err(error) => {
                                    tracing::error!(%error, path = %put_rel, "WebDAV PUT If-Match resource lookup failed");
                                    return internal_error();
                                }
                            }
                        }
                        _ => return internal_error(),
                    };
                    let failed = (!if_match_values.is_empty()
                        && !if_match_satisfied(
                            &if_match_values.join(", "),
                            cur.as_deref(),
                            exists,
                        ))
                        || (if_match_values.is_empty()
                            && if_unmodified_since.as_deref().is_some_and(|condition| {
                                modified_at.is_some_and(|modified| {
                                    if_unmodified_since_failed(condition, modified)
                                })
                            }))
                        || if_none_match.as_deref().is_some_and(|condition| {
                            if_none_match_satisfied(condition, cur.as_deref(), exists)
                        });
                    if failed {
                        return Response::builder()
                            .status(StatusCode::PRECONDITION_FAILED)
                            .body(Body::empty())
                            .unwrap();
                    }
                }
                let resp = put_op(
                    app_owned,
                    user_owned,
                    ns_owned,
                    uri_owned,
                    req.headers()
                        .get("if")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string),
                    write_condition,
                    put_body,
                )
                .await;
                if resp.status().is_success() {
                    if let (Some(app), Some(u)) = (
                        req.extensions().get::<WebdavApplication>(),
                        req.extensions().get::<vfiles_domain::types::User>(),
                    ) {
                        audit_write(
                            app,
                            "webdav.put",
                            percent_decode(req.uri().path()),
                            u,
                            ua.as_deref(),
                            vfiles_domain::types::AuditResult::Success,
                        );
                    }
                } else if resp.status() == StatusCode::CONFLICT || resp.status().as_u16() >= 500 {
                    // r17 失败面审计表记（409/5xx → Failure ✗ 423/412 拒绝 = warn 覆盖范围注 ✓）
                    if let (Some(app), Some(u)) = (
                        req.extensions().get::<WebdavApplication>(),
                        req.extensions().get::<vfiles_domain::types::User>(),
                    ) {
                        audit_write(
                            app,
                            "webdav.put",
                            percent_decode(req.uri().path()),
                            u,
                            req.headers()
                                .get("user-agent")
                                .and_then(|v| v.to_str().ok()),
                            vfiles_domain::types::AuditResult::Failure,
                        );
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
    axum::serve(
        listener,
        router(_app.clone()).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
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
mod overwrite_header_tests {
    use super::parse_overwrite_header;
    use axum::http::HeaderMap;

    #[test]
    fn accepts_only_one_valid_overwrite_value_and_defaults_to_true() {
        let mut headers = HeaderMap::new();
        assert_eq!(parse_overwrite_header(&headers), Some(true));

        headers.insert("overwrite", "T".parse().unwrap());
        assert_eq!(parse_overwrite_header(&headers), Some(true));
        headers.insert("overwrite", "t".parse().unwrap());
        assert_eq!(parse_overwrite_header(&headers), Some(true));
        headers.insert("overwrite", "F".parse().unwrap());
        assert_eq!(parse_overwrite_header(&headers), Some(false));
        headers.insert("overwrite", "f".parse().unwrap());
        assert_eq!(parse_overwrite_header(&headers), Some(false));
        headers.insert("overwrite", "x".parse().unwrap());
        assert_eq!(parse_overwrite_header(&headers), None);

        headers.insert("overwrite", "T".parse().unwrap());
        headers.append("overwrite", "F".parse().unwrap());
        assert_eq!(parse_overwrite_header(&headers), None);
    }
}

#[cfg(test)]
mod write_depth_tests {
    use super::parse_write_depth;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn parses_supported_depth_values_and_rejects_duplicates() {
        let headers = HeaderMap::new();
        assert_eq!(parse_write_depth(&headers), Ok(None));

        let mut headers = HeaderMap::new();
        headers.insert("depth", HeaderValue::from_static("Infinity"));
        assert_eq!(parse_write_depth(&headers), Ok(Some(true)));

        for value in ["0", "1"] {
            let mut headers = HeaderMap::new();
            headers.insert("depth", HeaderValue::from_str(value).unwrap());
            assert_eq!(parse_write_depth(&headers), Ok(Some(false)));
        }

        for value in ["infinity, 0", "invalid"] {
            let mut headers = HeaderMap::new();
            headers.insert("depth", HeaderValue::from_str(value).unwrap());
            assert_eq!(parse_write_depth(&headers), Err(()), "accepted {value:?}");
        }

        let mut headers = HeaderMap::new();
        headers.append("depth", HeaderValue::from_static("infinity"));
        headers.append("depth", HeaderValue::from_static("infinity"));
        assert_eq!(parse_write_depth(&headers), Err(()));
    }
}

#[cfg(test)]
mod write_tests {
    use super::{DestinationContext, destination_path, destination_path_for_request};
    use axum::http::{HeaderMap, HeaderValue, Uri, header};

    #[test]
    fn parses_destination_absolute_and_relative() {
        // 挂载点 = root（`/` ✓ 客户端 base 自配）；`/dav/` 前缀样 = 语义错配已正
        assert_eq!(
            destination_path("http://host/a/b.txt", ""),
            Some("a/b.txt".to_string())
        );
        assert_eq!(destination_path("/sub/x", ""), Some("sub/x".to_string()));
        assert_eq!(destination_path("no-leading-slash", ""), None);
        // r-new 嵌入形（mount 剥离三式 ✗ 与独立形并存守护断言）
        assert_eq!(
            destination_path("http://host/dav/a/b.txt", "/dav"),
            Some("a/b.txt".to_string())
        );
        assert_eq!(destination_path("/dav", "/dav"), Some(String::new()));
        assert_eq!(destination_path("/other/x", "/dav"), None);
        assert_eq!(
            destination_path("http://host/dav/a%20b/%E6%B1%89", "/dav"),
            Some("a b/汉".to_string())
        );
        assert_eq!(destination_path("/dav/file?version=1", "/dav"), None);
    }

    #[test]
    fn destination_absolute_uri_must_match_request_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("dav.example:8080"));
        let request_uri = Uri::from_static("/dav/source.txt");
        let context = DestinationContext::from_request(&request_uri, &headers);

        assert_eq!(
            destination_path_for_request(
                "http://DAV.example:8080/dav/target.txt",
                "/dav",
                &context,
            ),
            Some("target.txt".to_string())
        );
        assert_eq!(
            destination_path_for_request(
                "http://attacker.example:8080/dav/target.txt",
                "/dav",
                &context,
            ),
            None
        );
        assert_eq!(
            destination_path_for_request(
                "http://dav.example:8081/dav/target.txt",
                "/dav",
                &context,
            ),
            None
        );
        assert_eq!(
            destination_path_for_request("/dav/target.txt", "/dav", &context),
            Some("target.txt".to_string())
        );
    }

    #[test]
    fn destination_absolute_uri_must_match_known_request_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("dav.example"));
        let request_uri = Uri::from_static("http://dav.example/source.txt");
        let context = DestinationContext::from_request(&request_uri, &headers);

        assert_eq!(
            destination_path_for_request("http://dav.example/target.txt", "", &context),
            Some("target.txt".to_string())
        );
        assert_eq!(
            destination_path_for_request("https://dav.example/target.txt", "", &context),
            None
        );
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

#[cfg(test)]
mod href_tests {
    use crate::server::{child_prefix, encode_uri_path, entry_href, href_with_mount};

    #[test]
    fn root_children_have_single_slash() {
        // r204 真因回归守护 ✗✗ 空 rel 必须单斜杠（//x = gvfs 丢弃条目）
        assert_eq!(
            entry_href(&child_prefix(""), "根文件.txt", false),
            "/根文件.txt"
        );
        assert_eq!(entry_href(&child_prefix(""), "项目库", true), "/项目库/");
        assert!(!entry_href(&child_prefix(""), "x", false).starts_with("//"));
    }

    #[test]
    fn nested_children_keep_prefix() {
        assert_eq!(
            entry_href(&child_prefix("项目库"), "说明.md", false),
            "/项目库/说明.md"
        );
        assert_eq!(
            entry_href(&child_prefix("项目库/子"), "a", true),
            "/项目库/子/a/"
        );
    }

    #[test]
    fn href_encodes_uri_delimiters_spaces_and_unicode() {
        assert_eq!(
            href_with_mount("/dav", "/a b/汉字/#part?query%"),
            "/dav/a%20b/%E6%B1%89%E5%AD%97/%23part%3Fquery%25"
        );
        assert_eq!(encode_uri_path("/a&b/one+two"), "/a&b/one+two");
        assert_eq!(href_with_mount("", "/"), "/");
    }
}

#[cfg(test)]
mod if_token_tests {
    use crate::server::untagged_if_matches;

    #[test]
    fn evaluates_untagged_lists_with_and_or_not_and_entity_tags() {
        let expected = "opaquelocktoken:active";
        assert_eq!(
            untagged_if_matches(
                "(<opaquelocktoken:other>) (<opaquelocktoken:active>)",
                Some(expected),
                None,
            ),
            Some(true)
        );
        assert_eq!(
            untagged_if_matches(
                "(<urn:example:extension-token>) (<opaquelocktoken:active>)",
                Some(expected),
                None,
            ),
            Some(true),
            "an unknown valid state token in one alternative must not invalidate other lists"
        );
        assert_eq!(
            untagged_if_matches("(<urn:example:extension-token>)", Some(expected), None),
            Some(false),
            "unknown state tokens do not match the active lock"
        );
        assert_eq!(
            untagged_if_matches("(<opaquelocktoken:other>)", Some(expected), None,),
            Some(false)
        );
        assert_eq!(
            untagged_if_matches(
                "(<opaquelocktoken:active> [\"v1\"])",
                Some(expected),
                Some("\"v1\""),
            ),
            Some(true)
        );
        assert_eq!(
            untagged_if_matches(
                "(<opaquelocktoken:active> <opaquelocktoken:other>)",
                Some(expected),
                None,
            ),
            Some(false)
        );
        assert_eq!(
            untagged_if_matches(
                "(<opaquelocktoken:active> [\"stale\"])",
                Some(expected),
                Some("\"v1\""),
            ),
            Some(false)
        );
        assert_eq!(
            untagged_if_matches(
                "(Not <opaquelocktoken:other> <opaquelocktoken:active> [\"v1\"])",
                Some(expected),
                Some("\"v1\""),
            ),
            Some(true)
        );
        for negation in ["Not", "not", "NOT", "nOt"] {
            let header = format!("({negation} <urn:example:other> <opaquelocktoken:active>)");
            assert_eq!(
                untagged_if_matches(&header, Some(expected), None),
                Some(true),
                "Not keyword spelling {negation:?}"
            );
        }
        assert_eq!(
            untagged_if_matches(
                "(Not <opaquelocktoken:active> <opaquelocktoken:active>)",
                Some(expected),
                None,
            ),
            Some(false)
        );
        assert_eq!(
            untagged_if_matches(
                "</other-resource> (<opaquelocktoken:active>)",
                Some(expected),
                None,
            ),
            None
        );
        assert_eq!(
            untagged_if_matches("([\"v1\"])", None, Some("\"v1\"")),
            Some(true)
        );
        assert_eq!(
            untagged_if_matches("([\"old\"])", None, Some("\"v1\"")),
            Some(false)
        );
    }
}

#[cfg(test)]
mod tagged_if_tests {
    use super::if_header_matches_resource;

    #[test]
    fn applies_tagged_lists_to_their_own_source_and_destination() {
        let header = "<http://example.test/dav/source> (<opaquelocktoken:src>) </dav/dest> (<opaquelocktoken:dst> [\"dest-v1\"])";
        assert_eq!(
            if_header_matches_resource(header, "source", "/dav", Some("opaquelocktoken:src"), None,),
            Some(true)
        );
        assert_eq!(
            if_header_matches_resource(
                header,
                "dest",
                "/dav",
                Some("opaquelocktoken:dst"),
                Some("\"dest-v1\""),
            ),
            Some(true)
        );
        assert_eq!(
            if_header_matches_resource(
                header,
                "dest",
                "/dav",
                Some("opaquelocktoken:dst"),
                Some("\"dest-v2\""),
            ),
            Some(false)
        );
    }

    #[test]
    fn unrelated_tags_do_not_authorize_a_locked_resource() {
        let header = "</dav/source> (<opaquelocktoken:src>)";
        assert_eq!(
            if_header_matches_resource(header, "dest", "/dav", None, None),
            Some(true)
        );
        assert_eq!(
            if_header_matches_resource(header, "dest", "/dav", Some("opaquelocktoken:dst"), None,),
            Some(false)
        );
    }
}

#[cfg(test)]
mod range_tests {
    use super::{ByteRange, if_range_allows_range, parse_byte_range};
    use time::OffsetDateTime;

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

    #[test]
    fn if_range_accepts_matching_strong_tag_or_current_date() {
        let current = Some("\"current\"");
        let modified = Some(OffsetDateTime::from_unix_timestamp(1_000_000).unwrap());
        assert!(if_range_allows_range(None, current, modified));
        assert!(if_range_allows_range(
            Some("\"current\""),
            current,
            modified
        ));
        assert!(!if_range_allows_range(Some("\"stale\""), current, modified));
        assert!(!if_range_allows_range(
            Some("W/\"current\""),
            current,
            modified
        ));
        assert!(!if_range_allows_range(Some("\"current\""), None, modified));
        assert!(!if_range_allows_range(
            Some("Thu, 01 Jan 1970 00:00:00 GMT"),
            current,
            modified
        ));
        assert!(if_range_allows_range(
            Some("Mon, 12 Jan 1970 13:46:40 GMT"),
            None,
            modified
        ));
        assert!(if_range_allows_range(
            Some("Tue, 13 Jan 1970 13:46:40 GMT"),
            None,
            modified
        ));
        assert!(!if_range_allows_range(Some("not a date"), None, modified));
        assert!(!if_range_allows_range(
            Some("Tue, 13 Jan 1970 13:46:40 GMT"),
            None,
            None
        ));
    }
}

#[cfg(test)]
mod etag_tests {
    use super::{derive_etag, if_match_satisfied};

    #[test]
    fn if_match_uses_strong_entity_tag_comparison() {
        let current = "\"ab\"";
        assert!(if_match_satisfied("*", Some(current), true));
        assert!(if_match_satisfied("\"xy\", \"ab\"", Some(current), true));
        assert!(if_match_satisfied(
            "\"tag,with,commas\", \"ab\"",
            Some(current),
            true
        ));
        assert!(!if_match_satisfied("W/\"ab\"", Some(current), true));
        assert!(!if_match_satisfied("ab", Some(current), true));
        assert!(!if_match_satisfied("\"xy\"", Some(current), true));
        assert!(!if_match_satisfied(
            "\"ab\", malformed",
            Some(current),
            true
        ));
        assert!(!if_match_satisfied("*", None, false));
    }

    #[test]
    fn derive_is_quoted_hex() {
        let v = vfiles_domain::types::VersionId::from_uuid(uuid::Uuid::new_v4());
        let et = derive_etag(&v);
        assert!(et.starts_with('"') && et.ends_with('"'));
        assert_eq!(et.len(), 34);
    }
}

#[cfg(test)]
mod lockinfo_tests {
    use super::parse_lockinfo;

    #[test]
    fn rejects_multiple_lock_scope_choices_in_one_lockinfo() {
        let body = br#"<D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/><D:shared/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockinfo>"#;
        assert!(parse_lockinfo(body).is_err());
    }

    #[test]
    fn rejects_multiple_lockscope_elements() {
        let body = br#"<D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/></D:lockscope><D:lockscope><D:shared/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockinfo>"#;
        assert!(parse_lockinfo(body).is_err());
    }

    #[test]
    fn accepts_one_supported_lock_scope_and_type() {
        let body = br#"<D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockinfo>"#;
        assert!(matches!(
            parse_lockinfo(body).map(|lockinfo| lockinfo.scope),
            Ok(vfiles_domain::WebdavLockScope::Exclusive)
        ));
    }

    #[test]
    fn accepts_shared_write_scope() {
        let body = br#"<D:lockinfo xmlns:D="DAV:"><D:lockscope><D:shared/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockinfo>"#;
        assert!(matches!(
            parse_lockinfo(body).map(|lockinfo| lockinfo.scope),
            Ok(vfiles_domain::WebdavLockScope::Shared)
        ));
    }
}

#[cfg(test)]
mod lock_token_tests {
    use super::{parse_if_header, parse_lock_token_header, valid_state_token};

    #[test]
    fn accepts_one_coded_uri_and_rejects_malformed_lock_token_fields() {
        assert_eq!(
            parse_lock_token_header(" <opaquelocktoken:abc-123> "),
            Some("opaquelocktoken:abc-123".to_string())
        );
        assert!(valid_state_token("opaquelocktoken:abc-123?query"));
        assert!(valid_state_token("opaquelocktoken:abc-123%23encoded"));
        assert!(!valid_state_token("opaquelocktoken:abc-123#fragment"));
        assert!(parse_if_header("(<opaquelocktoken:abc-123#fragment>)").is_none());
        for malformed in [
            "opaquelocktoken:abc-123",
            "<<opaquelocktoken:abc-123>>",
            "<>",
            "<opaquelocktoken:abc 123>",
            "<opaquelocktoken:abc-123>>",
            "<opaquelocktoken:abc-123#fragment>",
        ] {
            assert_eq!(parse_lock_token_header(malformed), None, "{malformed:?}");
        }
    }
}

#[cfg(test)]
mod write_error_tests {
    use super::{copy_error_status, write_error_status};
    use axum::http::StatusCode;
    use vfiles_domain::DomainError;

    #[test]
    fn maps_domain_write_failures_to_protocol_statuses() {
        assert_eq!(
            write_error_status(&DomainError::NotFound {
                resource: "file".to_string()
            }),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            write_error_status(&DomainError::Forbidden),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            write_error_status(&DomainError::StorageQuotaExceeded),
            StatusCode::INSUFFICIENT_STORAGE
        );
        assert_eq!(
            write_error_status(&DomainError::Internal {
                message: "storage failure".to_string()
            }),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn copy_overwrite_precondition_maps_destination_conflicts_to_412() {
        let conflict = DomainError::PathConflict {
            message: "destination exists".to_string(),
        };
        assert_eq!(
            copy_error_status(&conflict, false),
            StatusCode::PRECONDITION_FAILED
        );
        assert_eq!(copy_error_status(&conflict, true), StatusCode::CONFLICT);
        assert_eq!(
            copy_error_status(&DomainError::StorageQuotaExceeded, true),
            StatusCode::INSUFFICIENT_STORAGE
        );
    }
}
