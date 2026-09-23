//! WebDAV multistatus XML（RFC 4918 §13）响应构造（r103 全实 ✓）。
//!
//! 207 Multi-Status：每资源一个 `<response>`（href + propfind 响应集：displayname /
//! resourcetype / getlastmodified / getcontentlength）。时间格式 = RFC1123（`time` crate）。

#![allow(dead_code)]

/// PROPFIND 请求体模式（RFC 4918 §9.1 ✗ r2 P0 协议精度）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropMode {
    /// 无体 / `<allprop/>` → 属性全集。
    All,
    /// `<propname/>` → 只出属性名（无值）。
    PropName,
    /// `<prop>…</prop>` → 精确集合（未知属性 = 404 propstat ✓ RFC 要求）。
    Names(Vec<String>),
}

/// 解析 PROPFIND 请求体（roxmltree DOM ✗ 非法/非 propfind → Err（调用方 400 ✓））。
pub fn parse_propfind_body(body: &str) -> Result<PropMode, ()> {
    if body.trim().is_empty() {
        return Ok(PropMode::All);
    }
    let doc = roxmltree::Document::parse(body).map_err(|_| ())?;
    let root = doc.root_element();
    if root.tag_name().name() != "propfind" {
        return Err(());
    }
    match root.children().find(|n| n.is_element()) {
        None => Ok(PropMode::All),
        Some(n) => match n.tag_name().name() {
            "allprop" => Ok(PropMode::All),
            "propname" => Ok(PropMode::PropName),
            "prop" => {
                let names: Vec<String> = n
                    .children()
                    .filter(|c| c.is_element())
                    .map(|c| c.tag_name().name().to_string())
                    .collect();
                if names.is_empty() {
                    return Err(());
                }
                Ok(PropMode::Names(names))
            }
            _ => Err(()),
        },
    }
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
}

/// 构造 207 Multi-Status 文档（XML 转义 ✓ 集合无 getcontentlength ✓）。
pub fn multistatus(items: &[PropResponse], mode: &PropMode) -> String {
    // r2 协议精度裁剪 ✗ 请求要什么给什么（All=全集 ✗ Names=交集+404 差集 ✗ PropName=只名）
    const SUPPORTED: [&str; 5] = [
        "displayname",
        "resourcetype",
        "getlastmodified",
        "getcontentlength",
        "getcontenttype",
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
                    .filter(|&sup| names.iter().any(|n| n.as_str() == sup))
                    .collect();
                // 404 块 = **请求了但支持集没有**（getetag 类 ✗ 方向错被 propmode 测试当场抓）
                let missing: Vec<&str> = names
                    .iter()
                    .map(|n| n.as_str())
                    .filter(|req| !SUPPORTED.contains(req))
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
                    out.push_str(&escape_xml(&item.displayname));
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
                // 其余 = propname 模式（只名无值）或占位（getcontentlength None 时跳过 ✓）
                other => {
                    let _ = other; // 值型属性在 propname 模式 = 空元素名
                }
            }
            // propname 模式：空值元素（RFC：只出名 ✓ 空体即名）
            if mode == &PropMode::PropName {
                out.push_str("<D:");
                out.push_str(name);
                out.push_str("/>");
            }
        }
        out.push_str("</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>");
        if !missing.is_empty() {
            // 未实现属性 = 404 propstat（RFC 4918 §9.1 合规 ✓ 客户端知道该属性不存在）
            out.push_str("<D:propstat><D:prop>");
            for name in &missing {
                out.push_str("<D:");
                out.push_str(name);
                out.push_str("/>");
            }
            out.push_str("</D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat>");
        }
        out.push_str("</D:response>");
    }
    out.push_str("\n</D:multistatus>");
    out
}

/// LOCK 响应体（lockdiscovery ✓ RFC 4918 §14.13 子集：exclusive write / depth 0 ✓）。
pub fn lock_response(token: &str, owner: &str, path: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:"><D:lockdiscovery><D:activelock>
<D:locktype><D:write/></D:locktype>
<D:lockscope><D:exclusive/></D:lockscope>
<D:depth>0</D:depth>
<D:owner>{owner}</D:owner>
<D:href>{href}</D:href>
<D:locktoken><D:href>{token}</D:href></D:locktoken>
<D:timeout>Infinite</D:timeout>
</D:activelock></D:lockdiscovery></D:prop>"#,
        owner = escape_xml(owner),
        href = escape_xml(&format!("/{path}")),
        token = escape_xml(token),
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
    use super::*;
    use super::PropMode;

    #[test]
    fn builds_multistatus_with_collections_and_files() {
        let xml = multistatus(&[
            PropResponse {
                href: "/dav/".into(),
                displayname: "root".into(),
                is_collection: true,
                getlastmodified: "Mon, 22 Sep 2026 19:20:00 GMT".into(),
                getcontentlength: None,
                getcontenttype: None,
            },
            PropResponse {
                href: "/dav/a&b.txt".into(),
                displayname: "a&b <txt>".into(),
                is_collection: false,
                getlastmodified: "Mon, 22 Sep 2026 19:21:00 GMT".into(),
                getcontentlength: Some(42),
                getcontenttype: Some("text/plain".into()),
            },
        ], &PropMode::All);
        assert!(xml.contains("<D:collection/>"));
        assert!(xml.contains("<D:getcontentlength>42</D:getcontentlength>"));
        assert!(xml.contains("a&amp;b &lt;txt&gt;"));
        assert!(xml.contains("/dav/a&amp;b.txt"));
        assert!(!xml.contains("<D:getcontentlength>") || xml.matches("<D:getcontentlength>").count() == 1);
    }

    #[test]
    fn escapes_all_xml_metacharacters() {
        assert_eq!(
            escape_xml(r#"&<>"'"#),
            "&amp;&lt;&gt;&quot;&apos;"
        );
    }
}

#[cfg(test)]
mod propmode_tests {
    use super::{parse_propfind_body, multistatus, PropMode, PropResponse};

    fn sample() -> Vec<PropResponse> {
        vec![PropResponse {
            href: "/f.txt".into(),
            displayname: "f.txt".into(),
            is_collection: false,
            getlastmodified: "Mon, 01 Jan 2026 00:00:00 +0000".into(),
            getcontentlength: Some(5),
            getcontenttype: Some("text/plain".into()),
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
            Ok(PropMode::Names(vec!["getcontentlength".into()]))
        );
        assert_eq!(parse_propfind_body("<broken"), Err(()));
        assert_eq!(
            parse_propfind_body(r#"<notpropfind><allprop/></notpropfind>"#),
            Err(())
        );
    }

    #[test]
    fn trims_to_requested_and_404s_unknown() {
        // r2 协议精度守护 ✗ 只要 getcontentlength → 只出它 + 未支持属性进 404 块
        let xml = multistatus(&sample(), &PropMode::Names(vec![
            "getcontentlength".into(),
            "displayname".into(),
            "getetag".into(), // 未实现 → 404 块
        ]));
        assert!(xml.contains("<D:getcontentlength>5</D:getcontentlength>"));
        assert!(xml.contains("<D:displayname>f.txt</D:displayname>"));
        assert!(!xml.contains("<D:getlastmodified>"), "未请求的不出现");
        assert!(xml.contains("404 Not Found"));
        assert!(xml.contains("<D:getetag/>"));
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
