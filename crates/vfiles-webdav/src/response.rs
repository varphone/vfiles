//! WebDAV multistatus XML（RFC 4918 §13）响应构造。
//! TODO(r103)：PROPFIND 响应体（207 Multi-Status ✓ href/displayname/getlastmodified/
//! getcontentlength/resourcetype ✓ 时间格式 = `time` crate RFC1123）。

#![allow(dead_code)]

/// 单个资源的属性行（占位 ✓ r103 实现）。
pub struct PropResponse {
    pub href: String,
}
