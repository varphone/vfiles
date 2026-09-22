//! WebDAV multistatus XML（RFC 4918 §13）响应构造（r103 全实 ✓）。
//!
//! 207 Multi-Status：每资源一个 `<response>`（href + propfind 响应集：displayname /
//! resourcetype / getlastmodified / getcontentlength）。时间格式 = RFC1123（`time` crate）。

#![allow(dead_code)]

/// 单资源属性（PROPFIND 单元 ✓）。
#[derive(Debug, Clone)]
pub struct PropResponse {
    pub href: String,
    pub displayname: String,
    pub is_collection: bool,
    /// RFC1123（如 `Mon, 22 Sep 2026 19:20:00 GMT`）。
    pub getlastmodified: String,
    pub getcontentlength: Option<u64>,
}

/// 构造 207 Multi-Status 文档（XML 转义 ✓ 集合无 getcontentlength ✓）。
pub fn multistatus(items: &[PropResponse]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">"#,
    );
    for item in items {
        out.push_str("\n<D:response><D:href>");
        out.push_str(&escape_xml(&item.href));
        out.push_str("</D:href><D:propstat><D:prop>");
        out.push_str("<D:displayname>");
        out.push_str(&escape_xml(&item.displayname));
        out.push_str("</D:displayname>");
        out.push_str(if item.is_collection {
            "<D:resourcetype><D:collection/></D:resourcetype>"
        } else {
            "<D:resourcetype/>"
        });
        out.push_str("<D:getlastmodified>");
        out.push_str(&escape_xml(&item.getlastmodified));
        out.push_str("</D:getlastmodified>");
        if let Some(len) = item.getcontentlength {
            out.push_str("<D:getcontentlength>");
            out.push_str(&len.to_string());
            out.push_str("</D:getcontentlength>");
        }
        out.push_str("</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>");
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

    #[test]
    fn builds_multistatus_with_collections_and_files() {
        let xml = multistatus(&[
            PropResponse {
                href: "/dav/".into(),
                displayname: "root".into(),
                is_collection: true,
                getlastmodified: "Mon, 22 Sep 2026 19:20:00 GMT".into(),
                getcontentlength: None,
            },
            PropResponse {
                href: "/dav/a&b.txt".into(),
                displayname: "a&b <txt>".into(),
                is_collection: false,
                getlastmodified: "Mon, 22 Sep 2026 19:21:00 GMT".into(),
                getcontentlength: Some(42),
            },
        ]);
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
