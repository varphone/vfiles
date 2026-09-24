//! WebDAV multistatus XML（RFC 4918 §13）响应构造（r103 全实 ✓）。
//!
//! 207 Multi-Status：每资源一个 `<response>`（href + propfind 响应集：displayname /
//! resourcetype / getlastmodified / getcontentlength）。时间格式 = RFC1123（`time` crate）。

#![allow(dead_code)]

/// PROPFIND 请求体模式（RFC 4918 §9.1 ✗ r2 P0 协议精度）。
/// DAV live properties that this server treats as read-only.
pub const PREDEFINED_READONLY: [&str; 9] = [
    "resourcetype",
    "getlastmodified",
    "getcontentlength",
    "getcontenttype",
    "getetag",      // r14 服务生成 ✗ PROPPATCH set → 403
    "creationdate", // r16 事实生成（建即定 ✗ 不可写）
    "owner",        // r16 属主事实（r109e 隔离下 ≡ 认证者 ✗ 不可写）
    "supportedlock",
    "lockdiscovery",
];

const PROPERTY_NAME_SEPARATOR: char = '\u{001f}';
// XML 1.0 forbids this character, so it cannot collide with a legacy text value.
const STORED_XML_PREFIX: &str = "\u{001f}vfiles-webdav-xml-v1:";

/// Storage key for an XML property name. XML names are identified by both parts.
pub fn property_key(namespace: Option<&str>, local_name: &str) -> String {
    format!(
        "{}{PROPERTY_NAME_SEPARATOR}{local_name}",
        namespace.unwrap_or("")
    )
}

pub fn is_dav_property(name: &str, local_name: &str) -> bool {
    name == property_key(Some("DAV:"), local_name)
}

pub fn is_predefined_readonly(name: &str) -> bool {
    PREDEFINED_READONLY
        .iter()
        .any(|local_name| is_dav_property(name, local_name))
}

pub(crate) fn stored_xml_value(value: &str) -> Option<&str> {
    value.strip_prefix(STORED_XML_PREFIX)
}

pub(crate) fn store_xml_element(node: roxmltree::Node<'_, '_>) -> String {
    format!("{STORED_XML_PREFIX}{}", serialize_xml_element(node))
}

fn serialize_xml_element(node: roxmltree::Node<'_, '_>) -> String {
    use std::collections::BTreeMap;

    fn add_namespace(uri: &str, map: &mut BTreeMap<String, String>) {
        if !uri.is_empty() && !map.contains_key(uri) {
            let prefix = format!("N{}", map.len());
            map.insert(uri.to_string(), prefix);
        }
    }

    fn collect_namespaces(node: roxmltree::Node<'_, '_>, map: &mut BTreeMap<String, String>) {
        if node.is_element() {
            if let Some(uri) = node.tag_name().namespace() {
                add_namespace(uri, map);
            }
            for attribute in node.attributes() {
                if let Some(uri) = attribute
                    .namespace()
                    .filter(|uri| *uri != "http://www.w3.org/XML/1998/namespace")
                {
                    add_namespace(uri, map);
                }
            }
        }
        for child in node.children() {
            collect_namespaces(child, map);
        }
    }

    fn write_node(
        node: roxmltree::Node<'_, '_>,
        namespaces: &BTreeMap<String, String>,
        out: &mut String,
        root: bool,
    ) {
        if node.is_text() {
            out.push_str(&escape_xml(node.text().unwrap_or_default()));
        } else if node.is_comment() {
            out.push_str("<!--");
            out.push_str(node.text().unwrap_or_default());
            out.push_str("-->");
        } else if node.is_element() {
            let tag = node.tag_name();
            let qname = tag
                .namespace()
                .and_then(|uri| namespaces.get(uri))
                .map(|prefix| format!("{prefix}:{}", tag.name()))
                .unwrap_or_else(|| tag.name().to_string());
            out.push('<');
            out.push_str(&qname);
            if root {
                for (uri, prefix) in namespaces {
                    out.push_str(" xmlns:");
                    out.push_str(prefix);
                    out.push_str("=\"");
                    out.push_str(&escape_xml(uri));
                    out.push('"');
                }
            }
            for attribute in node.attributes() {
                let attr_name = match attribute.namespace() {
                    Some("http://www.w3.org/XML/1998/namespace") => {
                        format!("xml:{}", attribute.name())
                    }
                    Some(uri) => format!("{}:{}", namespaces[uri], attribute.name()),
                    None => attribute.name().to_string(),
                };
                out.push(' ');
                out.push_str(&attr_name);
                out.push_str("=\"");
                out.push_str(&escape_xml(attribute.value()));
                out.push('"');
            }
            if node.children().next().is_none() {
                out.push_str("/>");
            } else {
                out.push('>');
                for child in node.children() {
                    write_node(child, namespaces, out, false);
                }
                out.push_str("</");
                out.push_str(&qname);
                out.push('>');
            }
        }
    }

    let mut namespaces = BTreeMap::new();
    collect_namespaces(node, &mut namespaces);
    let mut out = String::new();
    write_node(node, &namespaces, &mut out, true);
    out
}

fn property_parts(name: &str) -> (&str, &str) {
    name.split_once(PROPERTY_NAME_SEPARATOR)
        .unwrap_or(("DAV:", name))
}

fn append_property_name(out: &mut String, name: &str, self_closing: bool) {
    let (namespace, local_name) = property_parts(name);
    if namespace == "DAV:" {
        out.push_str("<D:");
        out.push_str(local_name);
        if self_closing {
            out.push_str("/>");
        } else {
            out.push('>');
        }
    } else if namespace.is_empty() {
        out.push('<');
        out.push_str(local_name);
        if self_closing {
            out.push_str("/>");
        } else {
            out.push('>');
        }
    } else {
        out.push_str("<X:");
        out.push_str(local_name);
        out.push_str(" xmlns:X=\"");
        out.push_str(&escape_xml(namespace));
        if self_closing {
            out.push_str("\"/>");
        } else {
            out.push_str("\">");
        }
    }
}

fn append_property_end(out: &mut String, name: &str) {
    let (namespace, local_name) = property_parts(name);
    if namespace == "DAV:" {
        out.push_str("</D:");
    } else if namespace.is_empty() {
        out.push_str("</");
    } else {
        out.push_str("</X:");
    }
    out.push_str(local_name);
    out.push('>');
}

fn canonical_stored_property_key(name: &str) -> String {
    if name.contains(PROPERTY_NAME_SEPARATOR) {
        name.to_string()
    } else {
        // Legacy rows stored only the local name; preserve their old DAV-style
        // rendering while new properties retain their namespace URI.
        property_key(Some("DAV:"), name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropMode {
    /// 无体 / `<allprop/>` → 属性全集。
    All,
    /// `<propname/>` → 只出属性名（无值）。
    PropName,
    /// `<prop>…</prop>` → 精确集合（未知属性 = 404 propstat ✓ RFC 要求）。
    Names(Vec<String>),
}

/// PROPPATCH 操作（RFC 4918 §9.2 ✓）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropOp {
    /// `<set><prop><name>value</name>`.
    Set { name: String, value: String },
    /// `<remove><prop><name/>`.
    Remove { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropPatchStatus {
    Ok,
    Forbidden,
    Conflict,
    FailedDependency,
    InternalServerError,
}

/// 解析 propertyupdate 请求体（roxmltree ✗ 按文档序收集 set/remove 操作）。
pub fn parse_propertyupdate(body: &str) -> Result<Vec<PropOp>, ()> {
    if body.trim().is_empty() {
        return Err(());
    }
    let doc = roxmltree::Document::parse(body).map_err(|_| ())?;
    let root = doc.root_element();
    if has_invalid_prefixed_namespace(root) {
        return Err(());
    }
    if root.tag_name().namespace() != Some("DAV:") || root.tag_name().name() != "propertyupdate" {
        return Err(());
    }
    let mut ops = Vec::new();
    for op in root.children().filter(|n| n.is_element()) {
        match op.tag_name().name() {
            "set" | "remove" if op.tag_name().namespace() == Some("DAV:") => {
                let is_set = op.tag_name().name() == "set";
                let prop = op.children().find(|n| {
                    n.is_element()
                        && n.tag_name().namespace() == Some("DAV:")
                        && n.tag_name().name() == "prop"
                });
                let Some(prop) = prop else {
                    return Err(());
                };
                for child in prop.children().filter(|c| c.is_element()) {
                    let name = property_key(child.tag_name().namespace(), child.tag_name().name());
                    if is_set {
                        let value = if is_dav_property(&name, "displayname") {
                            child.text().unwrap_or_default().to_string()
                        } else {
                            store_xml_element(child)
                        };
                        ops.push(PropOp::Set { name, value });
                    } else {
                        ops.push(PropOp::Remove { name });
                    }
                }
            }
            _ => return Err(()),
        }
    }
    if ops.is_empty() {
        return Err(());
    }
    Ok(ops)
}

/// PROPPATCH 207 响应（每操作一条 propstat，包含依赖失败状态）。
pub fn proppatch_multistatus(href: &str, results: &[(PropOp, PropPatchStatus)]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">"#,
    );
    out.push_str("\n<D:response><D:href>");
    out.push_str(&escape_xml(href));
    out.push_str("</D:href>");
    for (op, status) in results {
        match op {
            PropOp::Set { name, .. } | PropOp::Remove { name } => {
                out.push_str("<D:propstat><D:prop>");
                append_property_name(&mut out, name, true);
            }
        }
        out.push_str("</D:prop><D:status>HTTP/1.1 ");
        out.push_str(match status {
            PropPatchStatus::Ok => "200 OK",
            PropPatchStatus::Forbidden => "403 Forbidden",
            PropPatchStatus::Conflict => "409 Conflict",
            PropPatchStatus::FailedDependency => "424 Failed Dependency",
            PropPatchStatus::InternalServerError => "500 Internal Server Error",
        });
        out.push_str("</D:status></D:propstat>");
    }
    out.push_str("</D:response>\n</D:multistatus>");
    out
}

/// 解析 PROPFIND 请求体（roxmltree DOM ✗ 非法/非 propfind → Err（调用方 400 ✓））。
pub fn parse_propfind_body(body: &str) -> Result<PropMode, ()> {
    if body.trim().is_empty() {
        return Ok(PropMode::All);
    }
    let doc = roxmltree::Document::parse(body).map_err(|_| ())?;
    let root = doc.root_element();
    if has_invalid_prefixed_namespace(root) {
        return Err(());
    }
    if root.tag_name().namespace() != Some("DAV:") || root.tag_name().name() != "propfind" {
        return Err(());
    }
    match root.children().find(|n| n.is_element()) {
        None => Ok(PropMode::All),
        Some(n) if n.tag_name().namespace() == Some("DAV:") => match n.tag_name().name() {
            "allprop" => Ok(PropMode::All),
            "propname" => Ok(PropMode::PropName),
            "prop" => {
                let names: Vec<String> = n
                    .children()
                    .filter(|c| c.is_element())
                    .map(|c| property_key(c.tag_name().namespace(), c.tag_name().name()))
                    .collect();
                if names.is_empty() {
                    return Err(());
                }
                Ok(PropMode::Names(names))
            }
            _ => Err(()),
        },
        Some(_) => Err(()),
    }
}

fn has_invalid_prefixed_namespace(root: roxmltree::Node<'_, '_>) -> bool {
    root.descendants()
        .filter(|node| node.is_element())
        .any(|node| {
            node.namespaces()
                .any(|namespace| namespace.name().is_some() && namespace.uri().is_empty())
        })
}

/// 单资源属性（PROPFIND 单元 ✓）。
#[derive(Debug, Clone)]
pub struct PropResponse {
    pub href: String,
    pub displayname: String,
    pub is_collection: bool,
    /// RFC1123（如 `Mon, 22 Sep 2026 19:20:00 GMT`）。
    pub getlastmodified: String,
    pub getcontentlength: Option<u64>,
    /// r3：MIME 类型（P1 顺车 ✗ get_stream 已带 mime 白送；集合 = None ✓）。
    pub getcontenttype: Option<String>,
    /// r13 自定义属性（k/v ✗ 名 = local name（ns 简式记档）；PROPFIND 输出/PROPPATCH 写回 ✓）。
    pub custom: Vec<(String, String)>,
    /// r14 ETag（= current_version_id 派生 `"hex32"` ✗ r4 SQL 已查零新查询；
    /// None = 目录/无版本 → 与 length 同式跳过 ✓ 强 ETag 带引号 ✓）。
    pub getetag: Option<String>,
    /// r16 创建时间（RFC 3339 ISO ✗ ≠ getlastmodified 的 RFC1123 ✓ 恒有 String）。
    pub creationdate: String,
    /// r16 属主（r109e per-user 隔离下 ≡ 认证用户名恒等 = 零查询白捡 ✓
    /// 记档：未来共享 ns 语义需回查 namespaces.owner_user_id ✓ 真值源已在表 ✗ 0001:30）。
    pub owner: String,
    /// 当前资源覆盖的活动锁（包含同一资源的多个共享锁）。
    pub active_lock: Vec<ActiveLock>,
}

#[derive(Debug, Clone)]
pub struct ActiveLock {
    pub token: String,
    pub owner: String,
    pub timeout: String,
    pub depth_infinity: bool,
    pub scope: vfiles_domain::WebdavLockScope,
    pub root_href: String,
}

/// 构造 207 Multi-Status 文档（XML 转义 ✓ 集合无 getcontentlength ✓）。
pub fn multistatus(items: &[PropResponse], mode: &PropMode) -> String {
    // r2 协议精度裁剪 ✗ 请求要什么给什么（All=全集 ✗ Names=交集+404 差集 ✗ PropName=只名）
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
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">"#,
    );
    for item in items {
        let (wanted, missing): (Vec<&str>, Vec<&str>) = match mode {
            PropMode::All | PropMode::PropName => (SUPPORTED.to_vec(), Vec::new()),
            PropMode::Names(names) => {
                let wanted: Vec<&str> = SUPPORTED
                    .iter()
                    .copied()
                    .filter(|&sup| names.iter().any(|name| is_dav_property(name, sup)))
                    .collect();
                // 404 块 = 请求了但支持集与本资源自定义集都没有（r13 自定义并入 ✓ r2 方向义保留）
                let missing: Vec<&str> = names
                    .iter()
                    .map(|n| n.as_str())
                    .filter(|req| {
                        !SUPPORTED
                            .iter()
                            .any(|supported| is_dav_property(req, supported))
                            && !item
                                .custom
                                .iter()
                                .any(|(cn, _)| canonical_stored_property_key(cn) == *req)
                    })
                    .collect();
                (wanted, missing)
            }
        };
        out.push_str("\n<D:response><D:href>");
        out.push_str(&escape_xml(&item.href));
        out.push_str("</D:href><D:propstat><D:prop>");
        for name in wanted {
            match name {
                "displayname" if mode != &PropMode::PropName => {
                    out.push_str("<D:displayname>");
                    let stored = item
                        .custom
                        .iter()
                        .find(|(name, _)| {
                            canonical_stored_property_key(name)
                                == property_key(Some("DAV:"), "displayname")
                        })
                        .map(|(_, value)| value.as_str())
                        .unwrap_or(&item.displayname);
                    out.push_str(&escape_xml(stored));
                    out.push_str("</D:displayname>");
                }
                "resourcetype" if mode != &PropMode::PropName => {
                    out.push_str(if item.is_collection {
                        "<D:resourcetype><D:collection/></D:resourcetype>"
                    } else {
                        "<D:resourcetype/>"
                    });
                }
                "getlastmodified" if mode != &PropMode::PropName => {
                    out.push_str("<D:getlastmodified>");
                    out.push_str(&escape_xml(&item.getlastmodified));
                    out.push_str("</D:getlastmodified>");
                }
                "getcontentlength" if mode != &PropMode::PropName => {
                    if let Some(len) = item.getcontentlength {
                        out.push_str("<D:getcontentlength>");
                        out.push_str(&len.to_string());
                        out.push_str("</D:getcontentlength>");
                    }
                }
                "getcontenttype" if mode != &PropMode::PropName => {
                    if let Some(ct) = &item.getcontenttype {
                        out.push_str("<D:getcontenttype>");
                        out.push_str(&escape_xml(ct));
                        out.push_str("</D:getcontenttype>");
                    }
                }
                "getetag" if mode != &PropMode::PropName => {
                    if let Some(et) = &item.getetag {
                        out.push_str("<D:getetag>");
                        out.push_str(&escape_xml(et));
                        out.push_str("</D:getetag>");
                    }
                }
                "creationdate" if mode != &PropMode::PropName => {
                    out.push_str("<D:creationdate>");
                    out.push_str(&escape_xml(&item.creationdate));
                    out.push_str("</D:creationdate>");
                }
                "owner" if mode != &PropMode::PropName => {
                    out.push_str("<D:owner>");
                    out.push_str(&escape_xml(&item.owner));
                    out.push_str("</D:owner>");
                }
                "supportedlock" if mode != &PropMode::PropName => {
                    out.push_str("<D:supportedlock><D:lockentry><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockentry><D:lockentry><D:lockscope><D:shared/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockentry></D:supportedlock>");
                }
                "lockdiscovery" if mode != &PropMode::PropName => {
                    if item.active_lock.is_empty() {
                        out.push_str("<D:lockdiscovery/>");
                    } else {
                        out.push_str("<D:lockdiscovery>");
                        for lock in &item.active_lock {
                            out.push_str(
                                "<D:activelock><D:locktype><D:write/></D:locktype><D:lockscope>",
                            );
                            out.push_str(match lock.scope {
                                vfiles_domain::WebdavLockScope::Exclusive => "<D:exclusive/>",
                                vfiles_domain::WebdavLockScope::Shared => "<D:shared/>",
                            });
                            out.push_str("</D:lockscope><D:depth>");
                            out.push_str(if lock.depth_infinity { "infinity" } else { "0" });
                            out.push_str("</D:depth>");
                            append_lock_owner(&mut out, &lock.owner);
                            out.push_str("<D:timeout>");
                            out.push_str(&escape_xml(&lock.timeout));
                            out.push_str("</D:timeout><D:locktoken><D:href>");
                            out.push_str(&escape_xml(&lock.token));
                            out.push_str("</D:href></D:locktoken><D:lockroot><D:href>");
                            out.push_str(&escape_xml(&lock.root_href));
                            out.push_str("</D:href></D:lockroot></D:activelock>");
                        }
                        out.push_str("</D:lockdiscovery>");
                    }
                }
                // 其余 = propname 模式（只名无值）或占位（getcontentlength None 时跳过 ✓）
                other => {
                    let _ = other; // 值型属性在 propname 模式 = 空元素名
                }
            }
            // propname 模式：空值元素（RFC：只出名 ✓ 空体即名）
            if mode == &PropMode::PropName {
                append_property_name(&mut out, &property_key(Some("DAV:"), name), true);
            }
        }
        // r13 自定义属性输出（All = 全出 ✗ Names = 交集 ✗ PropName = 只名无值）
        for (cn, cv) in &item.custom {
            if canonical_stored_property_key(cn) == property_key(Some("DAV:"), "displayname") {
                continue;
            }
            let requested = match mode {
                PropMode::All => true,
                PropMode::PropName => true,
                PropMode::Names(names) => {
                    let stored_key = canonical_stored_property_key(cn);
                    names.contains(&stored_key)
                }
            };
            if requested {
                let property_name = canonical_stored_property_key(cn);
                if mode == &PropMode::PropName {
                    append_property_name(&mut out, &property_name, true);
                } else if let Some(xml) = stored_xml_value(cv) {
                    out.push_str(xml);
                } else {
                    append_property_name(&mut out, &property_name, false);
                    out.push_str(&escape_xml(cv));
                    append_property_end(&mut out, &property_name);
                }
            }
        }
        out.push_str("</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>");
        if !missing.is_empty() {
            // 未实现属性 = 404 propstat（RFC 4918 §9.1 合规 ✓ 客户端知道该属性不存在）
            out.push_str("<D:propstat><D:prop>");
            for name in &missing {
                append_property_name(&mut out, name, true);
            }
            out.push_str("</D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat>");
        }
        out.push_str("</D:response>");
    }
    out.push_str("\n</D:multistatus>");
    out
}

/// LOCK 响应体（lockdiscovery ✓ RFC 4918 §14.13 子集：exclusive write / depth 0 ✓）。
pub fn lock_response(
    token: &str,
    owner: &str,
    path: &str,
    timeout: &str,
    depth_infinity: bool,
    scope: vfiles_domain::WebdavLockScope,
) -> String {
    let depth = if depth_infinity { "infinity" } else { "0" };
    let owner_xml = stored_xml_value(owner)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("<D:owner>{}</D:owner>", escape_xml(owner)));
    let scope_xml = match scope {
        vfiles_domain::WebdavLockScope::Exclusive => "<D:exclusive/>",
        vfiles_domain::WebdavLockScope::Shared => "<D:shared/>",
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:"><D:lockdiscovery><D:activelock>
<D:locktype><D:write/></D:locktype>
<D:lockscope>{scope_xml}</D:lockscope>
<D:depth>{depth}</D:depth>
{owner_xml}
<D:href>{href}</D:href>
<D:locktoken><D:href>{token}</D:href></D:locktoken>
<D:lockroot><D:href>{href}</D:href></D:lockroot>
<D:timeout>{timeout}</D:timeout>
</D:activelock></D:lockdiscovery></D:prop>"#,
        owner_xml = owner_xml,
        scope_xml = scope_xml,
        href = escape_xml(path), // r-new 修双斜杠：caller 已传完整 href（含 mount ✗ 模板不自加 "/"）
        token = escape_xml(token),
        timeout = escape_xml(timeout),
        depth = depth,
    )
}

fn append_lock_owner(out: &mut String, owner: &str) {
    if let Some(xml) = stored_xml_value(owner) {
        out.push_str(xml);
    } else {
        out.push_str("<D:owner>");
        out.push_str(&escape_xml(owner));
        out.push_str("</D:owner>");
    }
}

/// RFC 4918 §9.10.3 reports a failed depth-infinity acquisition as a
/// multistatus when a descendant prevents the whole hierarchy from locking.
pub fn lock_conflict_response(request_href: &str, conflict_href: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
<D:response><D:href>{conflict}</D:href><D:status>HTTP/1.1 423 Locked</D:status></D:response>
<D:response><D:href>{request}</D:href><D:status>HTTP/1.1 424 Failed Dependency</D:status></D:response>
</D:multistatus>"#,
        conflict = escape_xml(conflict_href),
        request = escape_xml(request_href),
    )
}

/// XML 转义（& < > " ' ✓ href/displayname 注入安全）。
fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::PropMode;
    use super::*;

    #[test]
    fn builds_multistatus_with_collections_and_files() {
        let xml = multistatus(
            &[
                PropResponse {
                    href: "/dav/".into(),
                    displayname: "root".into(),
                    is_collection: true,
                    getlastmodified: "Mon, 22 Sep 2026 19:20:00 GMT".into(),
                    getcontentlength: None,
                    getcontenttype: None,
                    custom: Vec::new(),
                    getetag: None,
                    creationdate: "2026-09-23T00:00:00Z".into(),
                    owner: "tester".into(),
                    active_lock: Vec::new(),
                },
                PropResponse {
                    href: "/dav/a&b.txt".into(),
                    displayname: "a&b <txt>".into(),
                    is_collection: false,
                    getlastmodified: "Mon, 22 Sep 2026 19:21:00 GMT".into(),
                    getcontentlength: Some(42),
                    getcontenttype: Some("text/plain".into()),
                    custom: Vec::new(),
                    getetag: None,
                    creationdate: "2026-09-23T00:00:00Z".into(),
                    owner: "tester".into(),
                    active_lock: Vec::new(),
                },
            ],
            &PropMode::All,
        );
        assert!(xml.contains("<D:collection/>"));
        assert!(xml.contains("<D:getcontentlength>42</D:getcontentlength>"));
        assert!(xml.contains("a&amp;b &lt;txt&gt;"));
        assert!(xml.contains("/dav/a&amp;b.txt"));
        assert!(
            !xml.contains("<D:getcontentlength>")
                || xml.matches("<D:getcontentlength>").count() == 1
        );
    }

    #[test]
    fn escapes_all_xml_metacharacters() {
        assert_eq!(escape_xml(r#"&<>"'"#), "&amp;&lt;&gt;&quot;&apos;");
    }
}

#[cfg(test)]
mod propmode_tests {
    use super::{PropMode, PropResponse, multistatus, parse_propfind_body, property_key};

    fn sample() -> Vec<PropResponse> {
        vec![PropResponse {
            href: "/f.txt".into(),
            displayname: "f.txt".into(),
            is_collection: false,
            getlastmodified: "Mon, 01 Jan 2026 00:00:00 +0000".into(),
            getcontentlength: Some(5),
            getcontenttype: Some("text/plain".into()),
            custom: Vec::new(),
            getetag: None,
            creationdate: "2026-09-23T00:00:00Z".into(),
            owner: "tester".into(),
            active_lock: Vec::new(),
        }]
    }

    #[test]
    fn parses_all_forms() {
        assert_eq!(parse_propfind_body(""), Ok(PropMode::All));
        assert_eq!(
            parse_propfind_body(r#"<D:propfind xmlns:D="DAV:"><D:allprop/></D:propfind>"#),
            Ok(PropMode::All)
        );
        assert_eq!(
            parse_propfind_body(r#"<D:propfind xmlns:D="DAV:"><D:propname/></D:propfind>"#),
            Ok(PropMode::PropName)
        );
        assert_eq!(
            parse_propfind_body(
                r#"<D:propfind xmlns:D="DAV:"><D:prop><D:getcontentlength/></D:prop></D:propfind>"#
            ),
            Ok(PropMode::Names(vec![property_key(
                Some("DAV:"),
                "getcontentlength",
            )]))
        );
        assert_eq!(parse_propfind_body("<broken"), Err(()));
        assert_eq!(
            parse_propfind_body(
                r#"<D:propfind xmlns:D="DAV:"><D:prop><bar:foo xmlns:bar=""/></D:prop></D:propfind>"#
            ),
            Err(())
        );
        assert_eq!(
            parse_propfind_body(r#"<notpropfind><allprop/></notpropfind>"#),
            Err(())
        );
    }

    #[test]
    fn trims_to_requested_and_404s_unknown() {
        // r2 协议精度守护 ✗ 只要 getcontentlength → 只出它 + 未支持属性进 404 块
        let xml = multistatus(
            &sample(),
            &PropMode::Names(vec![
                property_key(Some("DAV:"), "getcontentlength"),
                property_key(Some("DAV:"), "displayname"),
                property_key(Some("DAV:"), "getlockdiscovery"),
            ]),
        );
        assert!(xml.contains("<D:getcontentlength>5</D:getcontentlength>"));
        assert!(xml.contains("<D:displayname>f.txt</D:displayname>"));
        assert!(!xml.contains("<D:getlastmodified>"), "未请求的不出现");
        assert!(xml.contains("404 Not Found"));
        assert!(xml.contains("<D:getlockdiscovery/>"));
        // 200 块与 404 块分立
        assert!(xml.contains("200 OK"));
    }

    #[test]
    fn propname_lists_only_names() {
        let xml = multistatus(&sample(), &PropMode::PropName);
        assert!(xml.contains("<D:displayname/>"));
        assert!(xml.contains("<D:resourcetype/>"));
        assert!(!xml.contains(">f.txt<"), "propname 不出值");
        assert!(!xml.contains(">5<"));
    }
}

#[cfg(test)]
mod proppatch_tests {
    use super::{
        PropOp, PropPatchStatus, is_dav_property, parse_propertyupdate, property_key,
        proppatch_multistatus, stored_xml_value,
    };

    #[test]
    fn parses_set_and_remove_in_order() {
        let body = r#"<D:propertyupdate xmlns:D="DAV:">
            <D:set><D:prop><D:displayname>新名字</D:displayname></D:prop></D:set>
            <D:remove><D:prop><D:getetag/></D:prop></D:remove>
        </D:propertyupdate>"#;
        let ops = parse_propertyupdate(body).unwrap();
        assert_eq!(
            ops,
            vec![
                PropOp::Set {
                    name: property_key(Some("DAV:"), "displayname"),
                    value: "新名字".into()
                },
                PropOp::Remove {
                    name: property_key(Some("DAV:"), "getetag")
                },
            ]
        );
    }

    #[test]
    fn rejects_malformed_or_empty() {
        assert!(parse_propertyupdate("").is_err());
        assert!(parse_propertyupdate("<broken").is_err());
        assert!(parse_propertyupdate(r#"<D:propfind xmlns:D="DAV:"/>"#).is_err());
        assert!(parse_propertyupdate(r#"<D:propertyupdate xmlns:D="DAV:"/>"#).is_err());
    }

    #[test]
    fn response_carries_per_op_status() {
        let xml = proppatch_multistatus(
            "/f.txt",
            &[
                (
                    PropOp::Set {
                        name: "displayname".into(),
                        value: "x".into(),
                    },
                    PropPatchStatus::Ok,
                ),
                (
                    PropOp::Set {
                        name: "getetag".into(),
                        value: "y".into(),
                    },
                    PropPatchStatus::Forbidden,
                ),
            ],
        );
        assert!(xml.contains("403 Forbidden"));
        assert!(xml.contains("200 OK"));
        assert!(xml.contains("<D:getetag/>"));
    }

    #[test]
    fn preserves_custom_property_namespace_and_does_not_alias_dav_names() {
        let body = r#"<D:propertyupdate xmlns:D="DAV:" xmlns:X="urn:example:props">
            <D:set><D:prop><X:displayname>custom title</X:displayname></D:prop></D:set>
        </D:propertyupdate>"#;
        let ops = parse_propertyupdate(body).unwrap();
        let PropOp::Set { name, .. } = &ops[0] else {
            panic!("set instruction expected");
        };
        assert_eq!(
            name,
            &property_key(Some("urn:example:props"), "displayname")
        );
        assert!(!is_dav_property(name, "displayname"));

        let xml = proppatch_multistatus("/f.txt", &[(ops[0].clone(), PropPatchStatus::Ok)]);
        assert!(xml.contains("<X:displayname xmlns:X=\"urn:example:props\"/>"));
    }

    #[test]
    fn preserves_dead_property_text_whitespace() {
        let body = "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:X=\"urn:example:props\"><D:set><D:prop><X:label>  spaced value\n </X:label></D:prop></D:set></D:propertyupdate>";
        let ops = parse_propertyupdate(body).expect("property update should parse");
        let PropOp::Set { name, value } = &ops[0] else {
            panic!("set instruction expected")
        };
        assert_eq!(name, &property_key(Some("urn:example:props"), "label"));
        let fragment = stored_xml_value(value).expect("serialized XML value");
        let parsed = roxmltree::Document::parse(fragment).unwrap();
        assert_eq!(parsed.root_element().text(), Some("  spaced value\n "));
    }

    #[test]
    fn preserves_null_namespace_without_empty_prefix_binding() {
        let body = r#"<D:propertyupdate xmlns:D="DAV:"><D:set><D:prop><nonamespace xmlns="">randomvalue</nonamespace></D:prop></D:set></D:propertyupdate>"#;
        let ops = parse_propertyupdate(body).unwrap();
        let PropOp::Set { value, .. } = &ops[0] else {
            panic!("set instruction expected")
        };
        let fragment = stored_xml_value(value).expect("serialized XML value");
        assert!(!fragment.contains("xmlns:N0=\"\""));
        let parsed = roxmltree::Document::parse(fragment).expect("null-namespace XML is valid");
        assert_eq!(parsed.root_element().tag_name().namespace(), None);
    }

    #[test]
    fn preserves_nested_dead_property_xml_and_attributes() {
        let body = r#"<D:propertyupdate xmlns:D="DAV:" xmlns:X="urn:outer" xmlns:Y="urn:inner">
            <D:set><D:prop><X:complex key="value">before<Y:item>inside</Y:item>after</X:complex></D:prop></D:set>
        </D:propertyupdate>"#;
        let ops = parse_propertyupdate(body).unwrap();
        let PropOp::Set { value, .. } = &ops[0] else {
            panic!("set instruction expected")
        };
        let fragment = stored_xml_value(value).expect("serialized XML value");
        let parsed = roxmltree::Document::parse(fragment).expect("valid serialized property XML");
        let root = parsed.root_element();
        assert_eq!(root.attribute("key"), Some("value"));
        assert_eq!(root.tag_name().namespace(), Some("urn:outer"));
        let item = root.children().find(|node| node.is_element()).unwrap();
        assert_eq!(item.tag_name().namespace(), Some("urn:inner"));
        assert_eq!(item.text(), Some("inside"));
    }
}
