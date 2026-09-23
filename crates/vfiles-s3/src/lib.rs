//! S3 兼容 API（协议族并列 crate ✗ round r1 目标首件 · 近期路线第 1 件）。
//!
//! 形态：独立端口（S3 path-style 寻址与前端根语义冲突 ✗ MinIO 同款 9000 独立端口直觉 ✓）+
//! 单虚拟桶 `default`（严格语义：非 default 桶 → NoSuchBucket ✓ 客户端列桶自配对）+
//! 认证 = SigV4（s3s 内建 ✗ env 静态凭证表支持只读策略与命名空间 slug 绑定；未设时随机
//! 生成 + warn 打印 = 零配置试用）。
//!
//! 映射：key = 默认 ns 根下相对路径（tree 展开为 flat keys ✗ 版本链天然 = ETag =
//! current_version_id hex 引号（与 WebDAV r14 同式 ✓ 跨协议一致））。
//!
//! r3（本轮：列表面的商业级完备）：
//! - `ListObjectsV2` **全语义**：prefix · delimiter→CommonPrefixes · continuation-token ·
//!   start-after · max-keys（尊重请求，上限 1000）· is-truncated · next-continuation-token ·
//!   key-count；`Object` 带 **size / last-modified / ETag**
//! - `ListObjects`（V1）：marker / delimiter / max-keys / next-marker / common-prefixes
//! - `GetObject` / `HeadObject`：**last-modified** + **HTTP Range → 206 / Content-Range**
//!   （s3s 见 content_range 自动置 206 ✗ `Range::check` 负责夹取与 416）
//!
//! 扩展能力：multipart upload · region 校验 · 大桶 SQL 分页。

use async_trait::async_trait;
use s3s::dto::{
    AbortMultipartUploadInput, AbortMultipartUploadOutput, Bucket, BucketLocationConstraint,
    BucketVersioningStatus, CommonPrefix, CompleteMultipartUploadInput,
    CompleteMultipartUploadOutput, CopyObjectInput, CopyObjectOutput, CopyObjectResult,
    CopyPartResult, CreateBucketInput, CreateBucketOutput, CreateMultipartUploadInput,
    CreateMultipartUploadOutput, DeleteBucketInput, DeleteBucketOutput, DeleteObjectInput,
    DeleteObjectOutput, DeleteObjectsInput, DeleteObjectsOutput, DeletedObject, ETagCondition,
    Error as S3DeleteError, GetBucketLocationInput, GetBucketLocationOutput,
    GetBucketVersioningInput, GetBucketVersioningOutput, GetObjectInput, GetObjectOutput,
    HeadBucketInput, HeadBucketOutput, HeadObjectInput, HeadObjectOutput, ListBucketsInput,
    ListBucketsOutput, ListMultipartUploadsInput, ListMultipartUploadsOutput,
    ListObjectVersionsInput, ListObjectVersionsOutput, ListObjectsInput, ListObjectsOutput,
    ListObjectsV2Input, ListObjectsV2Output, ListPartsInput, ListPartsOutput, MultipartUpload,
    Object, ObjectVersion, Owner, Part, PutBucketVersioningInput, PutBucketVersioningOutput,
    PutObjectInput, PutObjectOutput, StreamingBlob, Timestamp, UploadPartCopyInput,
    UploadPartCopyOutput, UploadPartInput, UploadPartOutput,
};
use s3s::{S3, S3Request, S3Response, S3Result};
use tokio::io::AsyncReadExt;

/// 默认（唯一）虚拟桶名。
pub const DEFAULT_BUCKET: &str = "default";
/// 对外通告的区域（`GetBucketLocation` ✗ 签名区域不校验 = 最大客户端兼容）。
pub const S3_REGION: &str = "us-east-1";
/// 单页上限（S3 硬上限）。
const MAX_KEYS_LIMIT: usize = 1000;
const MAX_S3_PART_SIZE: u64 = 5 * 1024 * 1024 * 1024;

/// Vfiles S3 实现（薄组装 ✗ 写面 = app 层 workspace/upload 同 WebDAV 同源链 ✓）。
pub struct VfilesS3 {
    pub workspace: std::sync::Arc<vfiles_app::DefaultWorkspaceService>,
    pub upload: vfiles_app::UploadService<
        vfiles_infra_sqlite::SqliteEntryRepo,
        vfiles_infra_sqlite::SqliteSnapshotRepo,
        vfiles_infra_sqlite::FsBlobStore,
        vfiles_infra_sqlite::FsUploadStore,
    >,
    pub entry_repo: std::sync::Arc<dyn vfiles_domain::EntryRepo + Send + Sync>,
    pub namespace: vfiles_domain::NamespaceId,
    pub owner: vfiles_domain::UserId,
    /// 只读凭证（`access:secret:ro` ✗ 变更类操作一律 AccessDenied）。
    pub readonly_keys: std::collections::HashSet<String>,
}

fn ok<T>(output: T) -> S3Result<S3Response<T>> {
    Ok(S3Response::new(output))
}

/// DomainErr → S3 错（NotFound/NoSuchKey / 其余 InternalError ✗ 消息带因）。
fn dom_err(e: vfiles_domain::DomainError) -> s3s::S3Error {
    match e {
        vfiles_domain::DomainError::NotFound { .. } => s3s::s3_error!(NoSuchKey, "No such key"),
        other => s3s::s3_error!(InternalError, "{}", other),
    }
}

fn norm(key: &str) -> vfiles_domain::DomainResult<vfiles_domain::NormalizedPath> {
    vfiles_domain::NormalizedPath::new(key.trim_matches('/')).map_err(|err| {
        vfiles_domain::DomainError::Validation {
            message: format!("invalid key: {err}"),
        }
    })
}

/// 对象元数据（列表/详情共用 ✗ 一次树遍历取全）。
#[derive(Debug, Clone)]
struct ObjMeta {
    key: String,
    size: u64,
    last_modified: Timestamp,
    etag: String,
}

fn obj_meta(m: vfiles_domain::types::EntryChildMeta) -> ObjMeta {
    ObjMeta {
        key: m.entry.path_norm.as_str().to_string(),
        size: m.size_bytes.unwrap_or(0),
        last_modified: Timestamp::from(m.entry.created_at),
        etag: m
            .entry
            .current_version_id
            .map(|v| v.to_string().replace('-', ""))
            .unwrap_or_default(),
    }
}

/// 流式列表收集器（按 key 升序喂入 ✗ 与存储解耦 = 单测可喂内存序列）。
///
/// 语义与 `build_entries` + `paginate` 等价（r20 起取代全量物化）：
/// prefix 过滤 → delimiter 折叠（连续同组去重）→ `after` 独占续页 → 收 `max+1` 判截断。
struct ListCollector<'a> {
    prefix: &'a str,
    delimiter: Option<&'a str>,
    after: Option<&'a str>,
    max: usize,
    last_prefix: Option<String>,
    entries: Vec<Listed>,
    done: bool,
}

impl<'a> ListCollector<'a> {
    fn new(
        prefix: &'a str,
        delimiter: Option<&'a str>,
        after: Option<&'a str>,
        max: usize,
    ) -> Self {
        Self {
            prefix,
            delimiter: delimiter.filter(|d| !d.is_empty()),
            after,
            max,
            last_prefix: None,
            entries: Vec::new(),
            done: false,
        }
    }

    /// 喂入一个对象；返回 `false` = 已收满（外层可停止翻页）。
    fn push(&mut self, o: ObjMeta) -> bool {
        if self.done {
            return false;
        }
        if !o.key.starts_with(self.prefix) {
            if o.key.as_str() < self.prefix {
                return true;
            }
            // SQL 从 prefix 字典序下界开始；有序键中的前缀匹配连续，首个不匹配即越界。
            self.done = true;
            return false;
        }
        let listed = match self.delimiter {
            Some(d) => {
                let rest = &o.key[self.prefix.len()..];
                match rest.find(d) {
                    Some(idx) => {
                        let p = format!("{}{}{}", self.prefix, &rest[..idx], d);
                        if self.last_prefix.as_deref() == Some(p.as_str()) {
                            return true;
                        }
                        self.last_prefix = Some(p.clone());
                        Listed::Prefix(p)
                    }
                    None => {
                        self.last_prefix = None;
                        Listed::Object(o)
                    }
                }
            }
            None => {
                self.last_prefix = None;
                Listed::Object(o)
            }
        };
        if let Some(a) = self.after
            && listed.key() <= a
        {
            return true;
        }
        self.entries.push(listed);
        if self.entries.len() > self.max {
            self.done = true;
        }
        !self.done
    }

    fn finish(mut self) -> (Vec<Listed>, bool) {
        let truncated = self.entries.len() > self.max;
        self.entries.truncate(self.max);
        (self.entries, truncated)
    }
}

/// 一页列表（SQL 分页拉取 ✗ **不物化整桶**）：返回 `(条目, 是否截断)`。
async fn list_page(
    repo: &std::sync::Arc<dyn vfiles_domain::EntryRepo + Send + Sync>,
    ns: &vfiles_domain::NamespaceId,
    prefix: &str,
    delimiter: Option<&str>,
    after: Option<&str>,
    max: usize,
) -> vfiles_domain::DomainResult<(Vec<Listed>, bool)> {
    const BATCH: u32 = 1000;
    let mut c = ListCollector::new(prefix, delimiter, after, max);
    // 续页游标：滚出条目单调不减 → 直接从 `after` 之后扫原始 key（不丢条目，见 r20 论证）
    let mut cursor: Option<String> = after.map(|a| a.to_string());
    while !c.done {
        let batch = repo
            .files_with_meta_page(ns, prefix, cursor.as_deref(), BATCH)
            .await?;
        if batch.is_empty() {
            break;
        }
        for m in batch {
            cursor = Some(m.entry.path_norm.as_str().to_string());
            if !c.push(obj_meta(m)) {
                break;
            }
        }
    }
    Ok(c.finish())
}

/// `encoding-type=url` 时把页内 key / common prefix / next 游标全部百分号编码。
fn url_encode_page(p: &mut Page) {
    for o in &mut p.contents {
        if let Some(k) = &o.key {
            o.key = Some(url_encode(k));
        }
    }
    for c in &mut p.prefixes {
        if let Some(v) = &c.prefix {
            c.prefix = Some(url_encode(v));
        }
    }
    if let Some(n) = &p.next {
        p.next = Some(url_encode(n));
    }
}

/// 收集结果 → `Page`（`next` = 本页末条 key ✗ 与应用 `after` 独占语义配对）。
fn page_from(entries: Vec<Listed>, truncated: bool) -> Page {
    let next = truncated
        .then(|| entries.last().map(|e| e.key().to_string()))
        .flatten();
    let mut contents = Vec::new();
    let mut prefixes = Vec::new();
    for e in entries {
        match e {
            Listed::Object(o) => contents.push(object_dto(&o)),
            Listed::Prefix(p) => prefixes.push(CommonPrefix {
                prefix: Some(p.clone()),
            }),
        }
    }
    Page {
        contents,
        prefixes,
        truncated,
        next,
    }
}

fn object_dto(o: &ObjMeta) -> Object {
    Object {
        key: Some(o.key.clone()),
        size: Some(o.size as i64),
        last_modified: Some(o.last_modified.clone()),
        e_tag: Some(s3s::dto::ETag::Strong(o.etag.clone())),
        ..Default::default()
    }
}

/// 列表条目：对象或 roll-up 的 common prefix（delimiter 折叠 ✗ AWS 语义）。
#[derive(Clone)]
enum Listed {
    Object(ObjMeta),
    Prefix(String),
}

impl Listed {
    fn key(&self) -> &str {
        match self {
            Listed::Object(o) => &o.key,
            Listed::Prefix(p) => p,
        }
    }
}

/// 过滤 + delimiter 折叠 + 排序（common prefix 去重后与对象统一按 key 升序）。
/// 参考实现（仅单测用 ✗ 生产走 `ListCollector` 流式）。
#[cfg(test)]
fn build_entries(all: &[ObjMeta], prefix: &str, delimiter: Option<&str>) -> Vec<Listed> {
    use std::collections::BTreeSet;
    let mut prefixes: BTreeSet<String> = BTreeSet::new();
    let mut objects: Vec<Listed> = Vec::new();
    for o in all {
        if !o.key.starts_with(prefix) {
            continue;
        }
        if let Some(d) = delimiter.filter(|d| !d.is_empty()) {
            let rest = &o.key[prefix.len()..];
            if let Some(idx) = rest.find(d) {
                prefixes.insert(format!("{prefix}{}{d}", &rest[..idx]));
                continue;
            }
        }
        objects.push(Listed::Object(o.clone()));
    }
    objects.extend(prefixes.into_iter().map(Listed::Prefix));
    objects.sort_by(|a, b| a.key().cmp(b.key()));
    objects
}

/// 分页切片（`after` 独占 ✗ continuation-token / start-after / marker 同语义）。
struct Page {
    contents: Vec<Object>,
    prefixes: Vec<CommonPrefix>,
    truncated: bool,
    next: Option<String>,
}

#[cfg(test)]
fn paginate(entries: Vec<Listed>, after: Option<&str>, max: usize) -> Page {
    let start = after
        .map(|a| {
            entries
                .iter()
                .position(|e| e.key() > a)
                .unwrap_or(entries.len())
        })
        .unwrap_or(0);
    let rest = &entries[start..];
    let truncated = rest.len() > max;
    let page = &rest[..max.min(rest.len())];
    let next = truncated
        .then(|| page.last().map(|e| e.key().to_string()))
        .flatten();
    let mut contents = Vec::new();
    let mut prefixes = Vec::new();
    for e in page {
        match e {
            Listed::Object(o) => contents.push(object_dto(o)),
            Listed::Prefix(p) => prefixes.push(CommonPrefix {
                prefix: Some(p.clone()),
            }),
        }
    }
    Page {
        contents,
        prefixes,
        truncated,
        next,
    }
}

/// 解析 max-keys（缺省 1000；负值 = InvalidArgument；上限夹到 1000）。
fn resolve_max_keys(input: Option<i32>) -> S3Result<usize> {
    match input {
        None => Ok(MAX_KEYS_LIMIT),
        Some(v) if v < 0 => Err(s3s::s3_error!(InvalidArgument, "max-keys must be >= 0")),
        Some(v) => Ok((v as usize).min(MAX_KEYS_LIMIT)),
    }
}

/// S3 `encoding-type=url` 的百分号编码（RFC 3986 ✗ 未保留字符直出，其余按字节 %XX）。
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            // `/` 保留（S3 列表里 key 的路径分隔符不编码 = 客户端无需特殊处理）
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 非空字符串 → Some（S3 空 delimiter/prefix 视作未设）。
fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}

/// 计算 Range 切片（None = 全量）。`check` 越界 → InvalidRange（416）。
fn resolve_range(
    range: Option<s3s::dto::Range>,
    size: u64,
) -> S3Result<Option<std::ops::Range<u64>>> {
    match range {
        None => Ok(None),
        Some(r) => r
            .check(size)
            .map(Some)
            .map_err(|_| s3s::s3_error!(InvalidRange, "range not satisfiable")),
    }
}

/// `StreamingBlob` → `AsyncRead`（流式直连 blob 存储 ✗ 大文件不再全量入内存）。
fn stream_reader(blob: StreamingBlob) -> impl tokio::io::AsyncRead + Send + Unpin {
    use futures::TryStreamExt;
    let stream = blob.map_err(|e| std::io::Error::other(e.to_string()));
    tokio_util::io::StreamReader::new(stream)
}

/// key → (父目录路径, 文件名)（与 WebDAV/S3 PUT 同式 ✗ 空名兜底 "upload"）。
fn split_key(full: &str) -> (String, String) {
    let (parent, filename) = match full.rsplit_once('/') {
        Some((dir, name)) => (dir.to_string(), name.to_string()),
        None => (String::new(), full.to_string()),
    };
    let filename = if filename.is_empty() {
        "upload".to_string()
    } else {
        filename
    };
    (parent, filename)
}

/// S3 uploadId 字符串 → `UploadId`。
fn parse_upload_id(s: &str) -> S3Result<vfiles_domain::UploadId> {
    vfiles_domain::UploadId::from_string(s)
        .map_err(|_| s3s::s3_error!(InvalidArgument, "invalid upload id"))
}

/// `x-amz-copy-source-if-*` 条件校验（不满足 → `PreconditionFailed` ✗ RFC 9110 §13 + S3 语义）。
fn check_copy_conditions(
    if_match: Option<&ETagCondition>,
    if_none_match: Option<&ETagCondition>,
    if_modified_since: Option<&Timestamp>,
    if_unmodified_since: Option<&Timestamp>,
    src_etag: &str,
    src_modified: &Timestamp,
) -> S3Result<()> {
    let matches = |c: &ETagCondition| match c {
        ETagCondition::Any => true,
        ETagCondition::ETag(e) => e.value() == src_etag,
    };
    if let Some(c) = if_match
        && !matches(c)
    {
        return Err(s3s::s3_error!(
            PreconditionFailed,
            "copy-source-if-match failed"
        ));
    }
    if let Some(c) = if_none_match
        && matches(c)
    {
        return Err(s3s::s3_error!(
            PreconditionFailed,
            "copy-source-if-none-match failed"
        ));
    }
    if let Some(t) = if_modified_since
        && src_modified <= t
    {
        return Err(s3s::s3_error!(
            PreconditionFailed,
            "copy-source-if-modified-since failed"
        ));
    }
    if let Some(t) = if_unmodified_since
        && src_modified > t
    {
        return Err(s3s::s3_error!(
            PreconditionFailed,
            "copy-source-if-unmodified-since failed"
        ));
    }
    Ok(())
}

/// 读条件（`If-Match` / `If-None-Match` / `If-Modified-Since` / `If-Unmodified-Since` ✗ RFC 9110 §13）。
///
/// 命中"未修改" → `NotModified`（HTTP 304 ✗ 不读正文 = 缓存路径省 IO）；其余不符 → `PreconditionFailed`。
fn check_get_conditions(
    etag: &str,
    last_modified: &Timestamp,
    if_match: Option<&ETagCondition>,
    if_none_match: Option<&ETagCondition>,
    if_modified_since: Option<&Timestamp>,
    if_unmodified_since: Option<&Timestamp>,
) -> S3Result<()> {
    let matches = |c: &ETagCondition| match c {
        ETagCondition::Any => true,
        ETagCondition::ETag(e) => e.value() == etag,
    };
    if let Some(c) = if_match
        && !matches(c)
    {
        return Err(s3s::s3_error!(PreconditionFailed, "if-match failed"));
    }
    if let Some(t) = if_unmodified_since
        && last_modified > t
    {
        return Err(s3s::s3_error!(
            PreconditionFailed,
            "if-unmodified-since failed"
        ));
    }
    if let Some(c) = if_none_match
        && matches(c)
    {
        return Err(s3s::s3_error!(NotModified, "if-none-match matched"));
    }
    if let Some(t) = if_modified_since
        && last_modified <= t
    {
        return Err(s3s::s3_error!(NotModified, "not modified since"));
    }
    Ok(())
}

/// 秒级时间相等（客户端往返的 `LastModifiedTime` 只到秒 ✗ 与库内高精度时间不可直接比）。
fn same_second(a: &Timestamp, b: &Timestamp) -> bool {
    let fmt = s3s::dto::TimestampFormat::HttpDate;
    let (mut ba, mut bb) = (Vec::new(), Vec::new());
    a.format(fmt, &mut ba).is_ok() && b.format(fmt, &mut bb).is_ok() && ba == bb
}

/// 目标条目条件（`If-Match` / `If-None-Match` ✗ 缺失 = 视为不存在）→ 不满足 `PreconditionFailed`。
fn check_dest_conditions(
    dest_etag: Option<&str>,
    if_match: Option<&ETagCondition>,
    if_none_match: Option<&ETagCondition>,
) -> S3Result<()> {
    let matched = |c: &ETagCondition| match (c, dest_etag) {
        (ETagCondition::Any, cur) => cur.is_some(),
        (ETagCondition::ETag(e), Some(cur)) => e.value() == cur,
        (ETagCondition::ETag(_), None) => false,
    };
    if let Some(c) = if_match
        && !matched(c)
    {
        return Err(s3s::s3_error!(PreconditionFailed, "if-match failed"));
    }
    if let Some(c) = if_none_match
        && matched(c)
    {
        return Err(s3s::s3_error!(PreconditionFailed, "if-none-match failed"));
    }
    Ok(())
}

/// 源条目的 `(ETag, LastModified)`（条件头用）。
async fn source_identity(
    backend: &VfilesS3,
    src_path: &vfiles_domain::NormalizedPath,
) -> S3Result<(String, Timestamp)> {
    let e = backend
        .entry_at(src_path)
        .await?
        .ok_or_else(|| s3s::s3_error!(NoSuchKey, "source key not found"))?;
    Ok((
        e.current_version_id
            .map(|v| v.to_string().replace('-', ""))
            .unwrap_or_default(),
        Timestamp::from(e.created_at),
    ))
}

/// `x-amz-copy-source-range` 解析成半开区间，避免范围复制时分配内容副本。
fn copy_range_bounds(spec: &str, len: u64) -> S3Result<std::ops::Range<u64>> {
    let raw = spec
        .trim()
        .strip_prefix("bytes=")
        .ok_or_else(|| s3s::s3_error!(InvalidArgument, "invalid copy source range"))?;
    let (a, b) = raw
        .split_once('-')
        .ok_or_else(|| s3s::s3_error!(InvalidArgument, "invalid copy source range"))?;
    let (start, end) = if a.is_empty() {
        // 后缀形 `-N` = 末尾 N 字节
        let n: u64 = b
            .parse()
            .map_err(|_| s3s::s3_error!(InvalidArgument, "invalid copy source range"))?;
        (len.saturating_sub(n), len.saturating_sub(1))
    } else {
        let start: u64 = a
            .parse()
            .map_err(|_| s3s::s3_error!(InvalidArgument, "invalid copy source range"))?;
        let end = if b.is_empty() {
            len.saturating_sub(1)
        } else {
            b.parse::<u64>()
                .map_err(|_| s3s::s3_error!(InvalidArgument, "invalid copy source range"))?
        };
        (start, end)
    };
    if len == 0 || start >= len || start > end {
        return Err(s3s::s3_error!(
            InvalidRange,
            "copy source range not satisfiable"
        ));
    }
    let end = end.min(len - 1);
    Ok(start..end + 1)
}

/// part 数据 MD5 十六进制（S3 `UploadPart` 返回的 ETag 形）。
fn md5_hex(data: &[u8]) -> String {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn decode_content_md5(value: Option<&str>) -> S3Result<Option<[u8; 16]>> {
    use base64::Engine;
    let Some(value) = value else { return Ok(None) };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| s3s::s3_error!(InvalidDigest, "Content-MD5 is not valid base64"))?;
    let digest: [u8; 16] = decoded
        .try_into()
        .map_err(|_| s3s::s3_error!(InvalidDigest, "Content-MD5 must decode to 16 bytes"))?;
    Ok(Some(digest))
}

fn decode_checksum_sha256(value: Option<&str>) -> S3Result<Option<[u8; 32]>> {
    use base64::Engine;
    let Some(value) = value else { return Ok(None) };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| s3s::s3_error!(InvalidDigest, "x-amz-checksum-sha256 is not valid base64"))?;
    let digest: [u8; 32] = decoded
        .try_into()
        .map_err(|_| s3s::s3_error!(InvalidDigest, "SHA256 checksum must decode to 32 bytes"))?;
    Ok(Some(digest))
}

fn decode_checksum_crc32(value: Option<&str>) -> S3Result<Option<u32>> {
    use base64::Engine;
    let Some(value) = value else { return Ok(None) };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| s3s::s3_error!(InvalidDigest, "x-amz-checksum-crc32 is not valid base64"))?;
    let bytes: [u8; 4] = decoded
        .try_into()
        .map_err(|_| s3s::s3_error!(InvalidDigest, "CRC32 checksum must decode to 4 bytes"))?;
    Ok(Some(u32::from_be_bytes(bytes)))
}

fn decode_checksum_crc32c(value: Option<&str>) -> S3Result<Option<u32>> {
    use base64::Engine;
    let Some(value) = value else { return Ok(None) };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| s3s::s3_error!(InvalidDigest, "x-amz-checksum-crc32c is not valid base64"))?;
    let bytes: [u8; 4] = decoded
        .try_into()
        .map_err(|_| s3s::s3_error!(InvalidDigest, "CRC32C checksum must decode to 4 bytes"))?;
    Ok(Some(u32::from_be_bytes(bytes)))
}

fn decode_checksum_crc64nvme(value: Option<&str>) -> S3Result<Option<u64>> {
    use base64::Engine;
    let Some(value) = value else { return Ok(None) };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| {
            s3s::s3_error!(
                InvalidDigest,
                "x-amz-checksum-crc64nvme is not valid base64"
            )
        })?;
    let bytes: [u8; 8] = decoded
        .try_into()
        .map_err(|_| s3s::s3_error!(InvalidDigest, "CRC64NVME checksum must decode to 8 bytes"))?;
    Ok(Some(u64::from_be_bytes(bytes)))
}

impl std::fmt::Debug for VfilesS3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // workspace/upload 等非 Debug ✗ 标准省内容式（-W missing-debug-implementations 清零）
        f.debug_struct("VfilesS3").finish_non_exhaustive()
    }
}

/// S3 用户元数据（`x-amz-meta-*`）在 entry 属性表中的前缀（复用 0006 `entry_properties`）。
const S3_META_PREFIX: &str = "s3-meta:";

impl VfilesS3 {
    /// 读条目的 S3 用户元数据（`x-amz-meta-*` → 响应头）。
    async fn load_metadata(
        &self,
        entry_id: &vfiles_domain::EntryId,
    ) -> S3Result<Option<s3s::dto::Metadata>> {
        let mut map = self
            .entry_repo
            .list_entry_properties(&[*entry_id])
            .await
            .map_err(dom_err)?;
        let mut out = s3s::dto::Metadata::new();
        for (k, v) in map.remove(entry_id).unwrap_or_default() {
            if let Some(name) = k.strip_prefix(S3_META_PREFIX) {
                out.insert(name.to_string(), v);
            }
        }
        Ok((!out.is_empty()).then_some(out))
    }

    /// 覆盖写用户元数据（先清旧 = 与 `MetadataDirective=REPLACE` 语义一致）。
    async fn store_metadata(
        &self,
        entry_id: &vfiles_domain::EntryId,
        md: Option<&s3s::dto::Metadata>,
    ) -> S3Result<()> {
        let mut map = self
            .entry_repo
            .list_entry_properties(&[*entry_id])
            .await
            .map_err(dom_err)?;
        for (k, _) in map.remove(entry_id).unwrap_or_default() {
            if k.starts_with(S3_META_PREFIX) {
                self.entry_repo
                    .remove_entry_property(entry_id, &k)
                    .await
                    .map_err(dom_err)?;
            }
        }
        if let Some(md) = md {
            for (k, v) in md {
                self.entry_repo
                    .set_entry_property(entry_id, &format!("{S3_META_PREFIX}{k}"), v)
                    .await
                    .map_err(dom_err)?;
            }
        }
        Ok(())
    }

    /// 路径 → entry（元数据读写用 ✗ 不存在则 None）。
    async fn entry_at(
        &self,
        path: &vfiles_domain::NormalizedPath,
    ) -> S3Result<Option<vfiles_domain::Entry>> {
        self.entry_repo
            .find_by_path(&self.namespace, path)
            .await
            .map_err(dom_err)
    }

    /// 目标路径当前 ETag（`current_version_id` hex ✗ 不存在 = None）。
    async fn etag_at(&self, path: &vfiles_domain::NormalizedPath) -> S3Result<Option<String>> {
        Ok(self
            .entry_at(path)
            .await?
            .and_then(|e| e.current_version_id)
            .map(|v| v.to_string().replace('-', "")))
    }

    /// 确认 multipart uploadId 属于当前凭证的命名空间、未过期且绑定请求中的 key。
    async fn validate_multipart_target(
        &self,
        upload_id: &vfiles_domain::UploadId,
        key: &str,
    ) -> S3Result<()> {
        let requested_path = norm(key)
            .map_err(|_| s3s::s3_error!(NoSuchUpload, "upload id does not identify this object"))?;
        let session =
            self.upload
                .get_upload_session(upload_id)
                .await
                .map_err(|error| match error {
                    vfiles_domain::DomainError::NotFound { .. }
                    | vfiles_domain::DomainError::UploadExpired => {
                        s3s::s3_error!(NoSuchUpload, "upload does not exist or has expired")
                    }
                    other => dom_err(other),
                })?;
        let session_key = if session.target_path_norm.as_str().is_empty() {
            session.filename.clone()
        } else {
            format!("{}/{}", session.target_path_norm.as_str(), session.filename)
        };
        if session.namespace_id != self.namespace
            || session.owner_user_id != self.owner
            || session_key != requested_path.as_str()
            || session.state != vfiles_domain::UploadState::Receiving
            || session.expires_at < time::OffsetDateTime::now_utc()
        {
            return Err(s3s::s3_error!(
                NoSuchUpload,
                "upload id does not identify this object"
            ));
        }
        Ok(())
    }

    /// 目标版本（`versionId` ✗ 无 → 最新）：返回 `(etag, last_modified, 透传给 open_file 的 commit)`。
    async fn resolve_version(
        &self,
        entry: &vfiles_domain::Entry,
        version_id: Option<&str>,
    ) -> S3Result<(String, Timestamp, Option<String>)> {
        let Some(vid) = version_id else {
            let etag = entry
                .current_version_id
                .map(|v| v.to_string().replace('-', ""))
                .unwrap_or_default();
            return Ok((etag, Timestamp::from(entry.created_at), None));
        };
        let version_id = vfiles_domain::VersionId::from_string(vid)
            .map_err(|_| s3s::s3_error!(NoSuchVersion, "no such version"))?;
        let ev = self
            .entry_repo
            .find_version(&version_id)
            .await
            .map_err(|_| s3s::s3_error!(NoSuchVersion, "no such version"))?;
        if ev.entry_id != entry.id {
            return Err(s3s::s3_error!(
                NoSuchVersion,
                "version does not belong to this key"
            ));
        }
        Ok((
            ev.id.to_string().replace('-', ""),
            Timestamp::from(ev.created_at),
            Some(vid.to_string()),
        ))
    }

    /// 变更类操作门控：命中只读凭证 → `AccessDenied`。
    fn require_write(&self, creds: Option<&s3s::auth::Credentials>) -> S3Result<()> {
        if let Some(c) = creds
            && self.readonly_keys.contains(&c.access_key)
        {
            return Err(s3s::s3_error!(AccessDenied, "this access key is read-only"));
        }
        Ok(())
    }
}

#[async_trait]
impl S3 for VfilesS3 {
    async fn list_buckets(
        &self,
        _req: S3Request<s3s::dto::ListBucketsInput>,
    ) -> S3Result<S3Response<ListBucketsOutput>> {
        let out = ListBucketsOutput {
            buckets: Some(vec![Bucket {
                name: Some(DEFAULT_BUCKET.to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };
        ok(out)
    }

    async fn list_objects_v2(
        &self,
        req: S3Request<ListObjectsV2Input>,
    ) -> S3Result<S3Response<ListObjectsV2Output>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let max = resolve_max_keys(input.max_keys)?;
        let prefix = input.prefix.clone().unwrap_or_default();
        let delimiter = non_empty(input.delimiter.clone());
        let after = input
            .continuation_token
            .clone()
            .or_else(|| input.start_after.clone());

        let (entries, truncated) = list_page(
            &self.entry_repo,
            &self.namespace,
            &prefix,
            delimiter.as_deref(),
            after.as_deref(),
            max,
        )
        .await
        .map_err(dom_err)?;
        let mut page = page_from(entries, truncated);
        // `fetch-owner=true` → 逐对象带 `Owner`（本部署内条目均属该命名空间属主）
        if input.fetch_owner.unwrap_or(false) {
            let owner = Owner {
                id: Some(self.owner.to_string()),
                display_name: None,
            };
            for o in &mut page.contents {
                o.owner = Some(owner.clone());
            }
        }
        let encode = input.encoding_type.as_ref().map(|e| e.as_str()) == Some("url");
        if encode {
            url_encode_page(&mut page);
        }
        let (prefix_out, delimiter_out) =
            if input.encoding_type.as_ref().map(|e| e.as_str()) == Some("url") {
                (
                    non_empty(Some(url_encode(&prefix))),
                    delimiter.as_deref().map(url_encode),
                )
            } else {
                (non_empty(Some(prefix.clone())), delimiter.clone())
            };
        let out = ListObjectsV2Output {
            name: Some(input.bucket),
            prefix: prefix_out,
            delimiter: delimiter_out,
            encoding_type: input.encoding_type.clone(),
            max_keys: Some(max as i32),
            key_count: Some((page.contents.len() + page.prefixes.len()) as i32),
            is_truncated: Some(page.truncated),
            continuation_token: input.continuation_token,
            next_continuation_token: page.next,
            contents: (!page.contents.is_empty()).then_some(page.contents),
            common_prefixes: (!page.prefixes.is_empty()).then_some(page.prefixes),
            ..Default::default()
        };
        ok(out)
    }

    /// 建桶（本网关只有唯一虚拟桶 ✗ 命名冲突按 AWS 语义回 409）。
    async fn create_bucket(
        &self,
        req: S3Request<CreateBucketInput>,
    ) -> S3Result<S3Response<CreateBucketOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket == DEFAULT_BUCKET {
            // AWS：桶已存在且归本账户 → 409 BucketAlreadyOwnedByYou
            return Err(s3s::s3_error!(
                BucketAlreadyOwnedByYou,
                "your account already owns this bucket"
            ));
        }
        Err(s3s::s3_error!(
            InvalidBucketName,
            "this gateway exposes a single bucket"
        ))
    }

    /// 删桶（空 → 409 拒绝本固定桶；非空 → AWS 同形 `BucketNotEmpty`）。
    async fn delete_bucket(
        &self,
        req: S3Request<DeleteBucketInput>,
    ) -> S3Result<S3Response<DeleteBucketOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        // 一次 LIMIT 1 查询即可判定空否（不物化整桶）
        let any = self
            .entry_repo
            .files_with_meta_page(&self.namespace, "", None, 1)
            .await
            .map_err(dom_err)?;
        if !any.is_empty() {
            return Err(s3s::s3_error!(
                BucketNotEmpty,
                "The bucket you tried to delete is not empty"
            ));
        }
        Err(s3s::s3_error!(
            InvalidBucketState,
            "this gateway's bucket is fixed and cannot be deleted"
        ))
    }

    /// 列出对象版本（本系统**确有版本历史** ✗ `entry_versions` ✗ 本实现无删除标记）。
    ///
    /// 语义对齐 AWS：key 升序 ✗ key 内**新版本在前** ✗ `max_keys` 计入 version 条目 ✗
    /// `key_marker` + `version_id_marker` 续页 ✗ delimiter 折叠（不重复投递）。
    async fn list_object_versions(
        &self,
        req: S3Request<ListObjectVersionsInput>,
    ) -> S3Result<S3Response<ListObjectVersionsOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let max = resolve_max_keys(input.max_keys)?;
        let prefix = input.prefix.clone().unwrap_or_default();
        let delimiter = non_empty(input.delimiter.clone());
        let key_marker = input.key_marker.clone();
        let vid_marker = input.version_id_marker.clone();
        let encode = input.encoding_type.as_ref().map(|e| e.as_str()) == Some("url");

        let mut out_versions: Vec<ObjectVersion> = Vec::new();
        let mut prefixes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut truncated = false;
        let mut next_key: Option<String> = None;
        let mut next_vid: Option<String> = None;
        // 最后一次输出的 (key, version_id) ✗ 截断时作为续页游标
        let mut last_emitted: Option<(String, String)> = None;

        // Walk indexed path pages and fetch version rows in batches. The former
        // implementation materialized every key in a bucket and issued one
        // version query per key, making a small page cost O(bucket size) memory
        // and O(number of keys) database round trips.
        let from = key_marker
            .as_deref()
            .filter(|marker| *marker > prefix.as_str())
            .unwrap_or(&prefix);
        let mut cursor: Option<String> = None;
        'outer: loop {
            let page = self
                .entry_repo
                .files_with_meta_page(&self.namespace, from, cursor.as_deref(), 256)
                .await
                .map_err(dom_err)?;
            if page.is_empty() {
                break;
            }
            cursor = page.last().map(|m| m.entry.path_norm.as_str().to_owned());
            let entry_ids: Vec<_> = page.iter().map(|m| m.entry.id).collect();
            let mut versions_by_entry: std::collections::HashMap<_, Vec<_>> =
                std::collections::HashMap::new();
            for version in self
                .entry_repo
                .find_versions_for_entries(&entry_ids)
                .await
                .map_err(dom_err)?
            {
                versions_by_entry
                    .entry(version.entry_id)
                    .or_default()
                    .push(version);
            }
            for m in page {
                let key = m.entry.path_norm.as_str().to_string();
                if !key.starts_with(&prefix) {
                    break 'outer;
                }
                if let Some(d) = &delimiter {
                    let rest = &key[prefix.len()..];
                    if let Some(idx) = rest.find(d.as_str()) {
                        let cp = format!("{}{}{}", prefix, &rest[..idx], d);
                        // 续页：已投递过的 common prefix 不重复
                        if let Some(km) = &key_marker
                            && &cp <= km
                        {
                            continue;
                        }
                        if !prefixes.contains(&cp) {
                            if out_versions.len() + prefixes.len() >= max {
                                truncated = true;
                                break 'outer;
                            }
                            prefixes.insert(cp.clone());
                            last_emitted = Some((cp, String::new()));
                        }
                        continue;
                    }
                }
                // 游标：key 小于 marker 跳过；key 等于 marker 时投递到 version marker 之后
                if let Some(km) = &key_marker
                    && &key < km
                {
                    continue;
                }
                let mut skipping = key_marker.as_ref() == Some(&key);
                let mut entry_versions = versions_by_entry.remove(&m.entry.id).unwrap_or_default();
                entry_versions.sort_by_key(|e| std::cmp::Reverse(e.version_no)); // 新版本在前（AWS 同形）
                for ev in entry_versions {
                    let vid = ev.id.to_string().replace('-', "");
                    if skipping {
                        if vid_marker.as_ref() == Some(&vid) {
                            skipping = false;
                        }
                        continue;
                    }
                    if out_versions.len() + prefixes.len() >= max {
                        truncated = true;
                        if let Some((k, v)) = &last_emitted {
                            next_key = Some(k.clone());
                            next_vid = Some(v.clone());
                        }
                        break 'outer;
                    }
                    out_versions.push(ObjectVersion {
                        key: Some(key.clone()),
                        version_id: Some(vid.clone()),
                        is_latest: Some(m.entry.current_version_id == Some(ev.id)),
                        size: Some(ev.size_bytes.as_u64() as i64),
                        last_modified: Some(Timestamp::from(ev.created_at)),
                        e_tag: Some(s3s::dto::ETag::Strong(vid.clone())),
                        ..Default::default()
                    });
                    last_emitted = Some((key.clone(), vid));
                }
            }
        }
        if encode {
            for v in &mut out_versions {
                if let Some(k) = &v.key {
                    v.key = Some(url_encode(k));
                }
                if let Some(vi) = &v.version_id {
                    v.version_id = Some(url_encode(vi));
                }
            }
            prefixes = prefixes.into_iter().map(|p| url_encode(&p)).collect();
            if let Some(k) = &next_key {
                next_key = Some(url_encode(k));
            }
        }
        let out = ListObjectVersionsOutput {
            name: Some(input.bucket),
            versions: (!out_versions.is_empty()).then_some(out_versions),
            common_prefixes: (!prefixes.is_empty()).then(|| {
                prefixes
                    .into_iter()
                    .map(|p| s3s::dto::CommonPrefix { prefix: Some(p) })
                    .collect()
            }),
            delimiter: input.delimiter,
            encoding_type: input.encoding_type,
            is_truncated: Some(truncated),
            key_marker: input.key_marker,
            max_keys: Some(max as i32),
            prefix: input.prefix,
            version_id_marker: input.version_id_marker,
            next_key_marker: truncated.then_some(next_key).flatten(),
            next_version_id_marker: truncated
                .then(|| next_vid.unwrap_or_default())
                .filter(|v| !v.is_empty()),
            ..Default::default()
        };
        ok(out)
    }

    /// 桶存在性探测（rclone / aws-cli 连接检查常用路径）。
    async fn head_bucket(
        &self,
        req: S3Request<HeadBucketInput>,
    ) -> S3Result<S3Response<HeadBucketOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        ok(HeadBucketOutput::default())
    }

    /// 桶区域（回 `us-east-1` ✗ 客户端可用任意 region 配置签名）。
    async fn get_bucket_location(
        &self,
        req: S3Request<GetBucketLocationInput>,
    ) -> S3Result<S3Response<GetBucketLocationOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        ok(GetBucketLocationOutput {
            location_constraint: Some(BucketLocationConstraint::from(S3_REGION.to_string())),
        })
    }

    /// 版本控制状态（本实现不启用 ✗ 无 `Status` = unversioned，AWS 未启用时同形）。
    async fn get_bucket_versioning(
        &self,
        req: S3Request<GetBucketVersioningInput>,
    ) -> S3Result<S3Response<GetBucketVersioningOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        // 本实现每次写入都产生版本 = 恒为 Enabled（与 AWS 同形应答）
        ok(GetBucketVersioningOutput {
            status: Some(BucketVersioningStatus::from(
                BucketVersioningStatus::ENABLED.to_string(),
            )),
            ..Default::default()
        })
    }

    /// 设版本控制状态（恒 Enabled ✗ Suspended/未指定 = 本实现无法满足，诚实拒绝）。
    async fn put_bucket_versioning(
        &self,
        req: S3Request<PutBucketVersioningInput>,
    ) -> S3Result<S3Response<PutBucketVersioningOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        match input
            .versioning_configuration
            .status
            .as_ref()
            .map(|s| s.as_str())
        {
            Some("Enabled") => ok(PutBucketVersioningOutput::default()),
            Some(_) => Err(s3s::s3_error!(
                InvalidArgument,
                "this gateway versions every write; only Enabled is supported"
            )),
            None => Err(s3s::s3_error!(
                InvalidArgument,
                "versioning status must be specified"
            )),
        }
    }

    async fn list_objects(
        &self,
        req: S3Request<ListObjectsInput>,
    ) -> S3Result<S3Response<ListObjectsOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let max = resolve_max_keys(input.max_keys)?;
        let prefix = input.prefix.clone().unwrap_or_default();
        let delimiter = non_empty(input.delimiter.clone());
        let marker = input.marker.clone();

        let (entries, truncated) = list_page(
            &self.entry_repo,
            &self.namespace,
            &prefix,
            delimiter.as_deref(),
            marker.as_deref(),
            max,
        )
        .await
        .map_err(dom_err)?;
        let mut page = page_from(entries, truncated);
        // V1 语义：恒带 `Owner`
        let owner = Owner {
            id: Some(self.owner.to_string()),
            display_name: None,
        };
        for o in &mut page.contents {
            o.owner = Some(owner.clone());
        }
        if input.encoding_type.as_ref().map(|e| e.as_str()) == Some("url") {
            url_encode_page(&mut page);
        }
        let out = ListObjectsOutput {
            name: Some(input.bucket),
            prefix: non_empty(Some(prefix)),
            delimiter,
            marker: non_empty(marker),
            max_keys: Some(max as i32),
            is_truncated: Some(page.truncated),
            next_marker: page.next,
            contents: (!page.contents.is_empty()).then_some(page.contents),
            common_prefixes: (!page.prefixes.is_empty()).then_some(page.prefixes),
            ..Default::default()
        };
        ok(out)
    }

    async fn get_object(
        &self,
        req: S3Request<GetObjectInput>,
    ) -> S3Result<S3Response<GetObjectOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let path = norm(&input.key).map_err(dom_err)?;
        // ETag = current_version_id hex 引号（与 WebDAV derive_etag 跨协议同式）
        let entry = self
            .entry_repo
            .find_by_path(&self.namespace, &path)
            .await
            .map_err(dom_err)?
            .ok_or_else(|| s3s::s3_error!(NoSuchKey, "No such key"))?;
        // Strong 变体序列化自附引号 ✗ 存裸 hex 防双引（etag.rs:19-24）
        // `versionId` 定向：etag/时间取该版本 ✗ raw_commit 透传 = 正文/mime/size 同版本
        let (etag, last_modified, raw_commit) = self
            .resolve_version(&entry, input.version_id.as_deref())
            .await?;
        // 读条件（命中 304 即不读正文 = 缓存路径省 IO）
        check_get_conditions(
            &etag,
            &last_modified,
            input.if_match.as_ref(),
            input.if_none_match.as_ref(),
            input.if_modified_since.as_ref(),
            input.if_unmodified_since.as_ref(),
        )?;
        let file = self
            .workspace
            .open_file(&self.namespace, &path, raw_commit.as_deref())
            .await
            .map_err(dom_err)?;
        let mut data = Vec::with_capacity(file.size_bytes as usize);
        let mut reader = file.reader;
        reader
            .read_to_end(&mut data)
            .await
            .map_err(|e| s3s::s3_error!(InternalError, "read failed: {}", e))?;
        let size = data.len() as u64;
        let slice = resolve_range(input.range, size)?;
        let (body, content_length, content_range) = match slice {
            Some(r) => {
                let bytes = data[r.start as usize..r.end as usize].to_vec();
                let cr = format!("bytes {}-{}/{}", r.start, r.end - 1, size);
                (bytes, (r.end - r.start) as i64, Some(cr))
            }
            None => (data, size as i64, None),
        };
        let metadata = self.load_metadata(&entry.id).await?;
        let out = GetObjectOutput {
            body: Some(StreamingBlob::from_bytes(body.into())),
            content_length: Some(content_length),
            content_type: file.mime_type,
            accept_ranges: Some("bytes".to_string()),
            content_range,
            e_tag: Some(s3s::dto::ETag::Strong(etag.clone())),
            last_modified: Some(last_modified),
            metadata,
            // 版本化桶恒回版本 id（本系统 ETag ≡ version id hex）
            version_id: (!etag.is_empty()).then_some(etag),
            ..Default::default()
        };
        ok(out)
    }

    async fn head_object(
        &self,
        req: S3Request<HeadObjectInput>,
    ) -> S3Result<S3Response<HeadObjectOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let path = norm(&input.key).map_err(dom_err)?;
        let entry = self
            .entry_repo
            .find_by_path(&self.namespace, &path)
            .await
            .map_err(dom_err)?
            .ok_or_else(|| s3s::s3_error!(NoSuchKey, "No such key"))?;
        // `versionId` 定向：etag/时间取该版本 ✗ raw_commit 透传 = 正文/mime/size 同版本
        let (etag, last_modified, raw_commit) = self
            .resolve_version(&entry, input.version_id.as_deref())
            .await?;
        // 读条件（命中 304 即不读正文 = 缓存路径省 IO）
        check_get_conditions(
            &etag,
            &last_modified,
            input.if_match.as_ref(),
            input.if_none_match.as_ref(),
            input.if_modified_since.as_ref(),
            input.if_unmodified_since.as_ref(),
        )?;
        let file = self
            .workspace
            .open_file(&self.namespace, &path, raw_commit.as_deref())
            .await
            .map_err(dom_err)?;
        let size = file.size_bytes;
        let slice = resolve_range(input.range, size)?;
        let (content_length, content_range) = match slice {
            Some(r) => (
                (r.end - r.start) as i64,
                Some(format!("bytes {}-{}/{}", r.start, r.end - 1, size)),
            ),
            None => (size as i64, None),
        };
        let metadata = self.load_metadata(&entry.id).await?;
        let out = HeadObjectOutput {
            content_length: Some(content_length),
            content_type: file.mime_type,
            accept_ranges: Some("bytes".to_string()),
            content_range,
            e_tag: Some(s3s::dto::ETag::Strong(etag.clone())),
            last_modified: Some(last_modified),
            metadata,
            // 版本化桶恒回版本 id（本系统 ETag ≡ version id hex）
            version_id: (!etag.is_empty()).then_some(etag),
            ..Default::default()
        };
        ok(out)
    }

    async fn put_object(
        &self,
        req: S3Request<PutObjectInput>,
    ) -> S3Result<S3Response<PutObjectOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let path = norm(&input.key).map_err(dom_err)?;
        let expected_md5 = decode_content_md5(input.content_md5.as_deref())?;
        let expected_sha256 = decode_checksum_sha256(input.checksum_sha256.as_deref())?;
        let expected_crc32 = decode_checksum_crc32(input.checksum_crc32.as_deref())?;
        let expected_crc32c = decode_checksum_crc32c(input.checksum_crc32c.as_deref())?;
        let expected_crc64nvme = decode_checksum_crc64nvme(input.checksum_crc64nvme.as_deref())?;
        let expected_sha256_hex = expected_sha256.map(hex::encode);
        let response_checksum_sha256 = input.checksum_sha256.clone();
        let response_checksum_crc32 = input.checksum_crc32.clone();
        let response_checksum_crc32c = input.checksum_crc32c.clone();
        let response_checksum_crc64nvme = input.checksum_crc64nvme.clone();
        // parent/filename 拆（WebDAV put_file 同式 ✗ init=父+名）
        // 条件写（`If-Match` / `If-None-Match` ✗ S3 现代并发控制）
        let cur = self.etag_at(&path).await?;
        check_dest_conditions(
            cur.as_deref(),
            input.if_match.as_ref(),
            input.if_none_match.as_ref(),
        )?;
        let (parent_str, filename) = split_key(path.as_str());
        let parent = vfiles_domain::NormalizedPath::new(&parent_str)
            .map_err(|e| s3s::s3_error!(InvalidArgument, "{}", e))?;
        let session = match input.content_length {
            Some(length) if length < 0 => {
                return Err(s3s::s3_error!(InvalidArgument, "negative Content-Length"));
            }
            Some(length) => self
                .upload
                .init_upload(
                    &self.namespace,
                    &parent,
                    &filename,
                    length as u64,
                    input.content_type.as_deref(),
                    None,
                    &self.owner,
                )
                .await
                .map_err(dom_err)?,
            None => self
                .upload
                .init_stream_upload_unknown_size(
                    &self.namespace,
                    &parent,
                    &filename,
                    input.content_type.as_deref(),
                    &self.owner,
                )
                .await
                .map_err(dom_err)?,
        };
        let blob = input
            .body
            .unwrap_or_else(|| StreamingBlob::from_bytes(Default::default()));
        let reader = stream_reader(blob);
        let result = if input.content_length.is_some() {
            self.upload
                .complete_upload_from_stream_with_md5(
                    &session.upload_id,
                    expected_sha256_hex.as_deref(),
                    expected_md5,
                    expected_crc32,
                    expected_crc32c,
                    expected_crc64nvme,
                    Some("S3 PUT"),
                    Box::new(reader),
                )
                .await
        } else {
            self.upload
                .complete_upload_from_stream_unknown_size_with_md5(
                    &session.upload_id,
                    expected_sha256_hex.as_deref(),
                    expected_md5,
                    expected_crc32,
                    expected_crc32c,
                    expected_crc64nvme,
                    Some("S3 PUT"),
                    Box::new(reader),
                )
                .await
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if let Err(cancel_error) = self.upload.cancel_upload(&session.upload_id).await {
                    tracing::warn!(error = %cancel_error, upload_id = %session.upload_id, "S3：清理失败 PUT 会话失败");
                }
                return Err(match error {
                    vfiles_domain::DomainError::BlobChecksumMismatch => {
                        s3s::s3_error!(BadDigest, "uploaded object checksum did not match")
                    }
                    other => dom_err(other),
                });
            }
        };
        let etag = result.version.id.to_string().replace('-', "");
        // 用户元数据（`x-amz-meta-*`）落 entry 属性；覆盖写 = 清旧
        if let Some(entry) = self.entry_at(&path).await? {
            self.store_metadata(&entry.id, input.metadata.as_ref())
                .await?;
        }
        let out = PutObjectOutput {
            e_tag: Some(s3s::dto::ETag::Strong(etag)),
            checksum_sha256: response_checksum_sha256,
            checksum_crc32: response_checksum_crc32,
            checksum_crc32c: response_checksum_crc32c,
            checksum_crc64nvme: response_checksum_crc64nvme,
            size: Some(result.version.size_bytes.as_u64() as i64),
            ..Default::default()
        };
        ok(out)
    }

    /// 服务端复制（`aws s3 cp s3://…/a s3://…/b` / rclone 同 remote copy 路径）。
    ///
    /// 同桶内读源字节 → 走上传链落目标（`MetadataDirective=REPLACE` 时用请求 ContentType，
    /// 否则沿用源 MIME ✗ 版本历史在目标侧另起）。
    async fn copy_object(
        &self,
        req: S3Request<CopyObjectInput>,
    ) -> S3Result<S3Response<CopyObjectOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let (src_bucket, src_key) = match &input.copy_source {
            s3s::dto::CopySource::Bucket { bucket, key, .. } => {
                (bucket.to_string(), key.to_string())
            }
            _ => {
                return Err(s3s::s3_error!(
                    InvalidArgument,
                    "only <bucket>/<key> copy sources are supported"
                ));
            }
        };
        if src_bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "source bucket not found"));
        }
        let src_path = norm(&src_key).map_err(dom_err)?;
        let (src_etag, src_mtime) = source_identity(self, &src_path).await?;
        check_copy_conditions(
            input.copy_source_if_match.as_ref(),
            input.copy_source_if_none_match.as_ref(),
            input.copy_source_if_modified_since.as_ref(),
            input.copy_source_if_unmodified_since.as_ref(),
            &src_etag,
            &src_mtime,
        )?;
        let content = self
            .workspace
            .open_file(&self.namespace, &src_path, None)
            .await
            .map_err(dom_err)?;
        let replace = input
            .metadata_directive
            .clone()
            .map(|m| std::borrow::Cow::from(m) == "REPLACE")
            .unwrap_or(false);
        let ctype = if replace {
            input.content_type.clone()
        } else {
            content
                .mime_type
                .clone()
                .or_else(|| input.content_type.clone())
        };
        let dst_path = norm(&input.key).map_err(dom_err)?;
        // 目标条件（`If-Match` / `If-None-Match` 针对**目标**，与 copy-source 条件相区分）
        let cur = self.etag_at(&dst_path).await?;
        check_dest_conditions(
            cur.as_deref(),
            input.if_match.as_ref(),
            input.if_none_match.as_ref(),
        )?;
        let (parent_str, filename) = split_key(dst_path.as_str());
        let parent = vfiles_domain::NormalizedPath::new(&parent_str)
            .map_err(|e| s3s::s3_error!(InvalidArgument, "{}", e))?;
        let session = self
            .upload
            .init_upload(
                &self.namespace,
                &parent,
                &filename,
                content.size_bytes,
                ctype.as_deref(),
                None,
                &self.owner,
            )
            .await
            .map_err(dom_err)?;
        let result = self
            .upload
            .complete_upload_from_stream(
                &session.upload_id,
                None,
                Some("S3 COPY"),
                Box::new(content.reader),
            )
            .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if let Err(cancel_error) = self.upload.cancel_upload(&session.upload_id).await {
                    tracing::warn!(error = %cancel_error, upload_id = %session.upload_id, "S3：清理失败 COPY 会话失败");
                }
                return Err(dom_err(error));
            }
        };
        let etag = result.version.id.to_string().replace('-', "");
        // 用户元数据：`REPLACE` = 取请求；否则（COPY）= 抄源条目
        let md = if replace {
            input.metadata.clone()
        } else {
            match self.entry_at(&src_path).await? {
                Some(e) => self.load_metadata(&e.id).await?,
                None => None,
            }
        };
        if let Some(entry) = self.entry_at(&dst_path).await? {
            self.store_metadata(&entry.id, md.as_ref()).await?;
        }
        let out = CopyObjectOutput {
            copy_object_result: Some(CopyObjectResult {
                e_tag: Some(s3s::dto::ETag::Strong(etag)),
                last_modified: Some(Timestamp::from(result.version.created_at)),
                ..Default::default()
            }),
            ..Default::default()
        };
        ok(out)
    }

    /// 批量删除（`aws s3 rm --recursive` / `rclone sync --delete` 路径 ✗ 逐键幂等）。
    async fn delete_objects(
        &self,
        req: S3Request<DeleteObjectsInput>,
    ) -> S3Result<S3Response<DeleteObjectsOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let quiet = input.delete.quiet.unwrap_or(false);
        let keys: Vec<String> = input.delete.objects.iter().map(|o| o.key.clone()).collect();
        let mut deleted: Vec<DeletedObject> = Vec::new();
        let mut errors: Vec<S3DeleteError> = Vec::new();

        // 逐键点查存在性（`delete_entries` 遇缺失即整体 NotFound ✗ 必须先分区），
        // 存在者一次批量删（单快照），缺失者按 S3 幂等语义直接记 deleted。
        let mut existing: Vec<vfiles_domain::NormalizedPath> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut per_key_err: Vec<Option<String>> = vec![None; keys.len()];
        // 逐键条件（`ETag` / `LastModifiedTime` / `Size` ✗ r29/r31 并发删；不满足只拒该键）
        let want_etag: Vec<Option<String>> = input
            .delete
            .objects
            .iter()
            .map(|o| o.e_tag.as_ref().map(|e| e.value().to_string()))
            .collect();
        let want_mtime: Vec<Option<Timestamp>> = input
            .delete
            .objects
            .iter()
            .map(|o| o.last_modified_time.clone())
            .collect();
        let want_size: Vec<Option<i64>> = input.delete.objects.iter().map(|o| o.size).collect();
        // 父目录 → 子项 meta 缓存（`size` 条件需要版本大小 ✗ 避免逐键重复查询）
        let mut size_cache: std::collections::HashMap<
            String,
            std::collections::HashMap<String, u64>,
        > = std::collections::HashMap::new();
        for (i, k) in keys.iter().enumerate() {
            match norm(k) {
                Ok(p) if !p.as_str().is_empty() => {
                    match self.entry_repo.find_by_path(&self.namespace, &p).await {
                        Ok(Some(entry)) => {
                            if let Some(want) = &want_etag[i] {
                                let cur = entry
                                    .current_version_id
                                    .map(|v| v.to_string().replace('-', ""));
                                if cur.as_deref() != Some(want.as_str()) {
                                    per_key_err[i] = Some("PreconditionFailed".to_string());
                                    continue;
                                }
                            }
                            if let Some(want) = &want_mtime[i]
                                && !same_second(&Timestamp::from(entry.created_at), want)
                            {
                                per_key_err[i] = Some("PreconditionFailed".to_string());
                                continue;
                            }
                            if let Some(want) = want_size[i] {
                                let (parent_str, _) =
                                    p.as_str().rsplit_once('/').unwrap_or(("", p.as_str()));
                                let sizes = match size_cache.get(parent_str) {
                                    Some(m) => m.clone(),
                                    None => {
                                        let parent = vfiles_domain::NormalizedPath::new(parent_str)
                                            .map_err(|e| {
                                                s3s::s3_error!(InvalidArgument, "{}", e)
                                            })?;
                                        let m: std::collections::HashMap<String, u64> = self
                                            .entry_repo
                                            .children_with_meta(&self.namespace, &parent)
                                            .await
                                            .map_err(dom_err)?
                                            .into_iter()
                                            .map(|c| {
                                                (
                                                    c.entry.path_norm.as_str().to_string(),
                                                    c.size_bytes.unwrap_or(0),
                                                )
                                            })
                                            .collect();
                                        size_cache.insert(parent_str.to_string(), m.clone());
                                        m
                                    }
                                };
                                if sizes.get(p.as_str()).copied() != Some(want as u64) {
                                    per_key_err[i] = Some("PreconditionFailed".to_string());
                                    continue;
                                }
                            }
                            if seen.insert(p.as_str().to_string()) {
                                existing.push(p);
                            }
                        }
                        // 缺失 + 带条件 = 条件不可满足（幂等语义仅对**无条件**删适用）
                        Ok(None) => {
                            if want_etag[i].is_some() {
                                per_key_err[i] = Some("PreconditionFailed".to_string());
                            }
                        }
                        Err(e) => per_key_err[i] = Some(e.to_string()),
                    }
                }
                Ok(_) => per_key_err[i] = Some("invalid key".to_string()),
                Err(e) => per_key_err[i] = Some(e.to_string()),
            }
        }

        let batch_failed = if existing.is_empty() {
            false
        } else {
            match self
                .workspace
                .delete_entries(
                    &self.namespace,
                    &existing,
                    Some("S3 DeleteObjects"),
                    &self.owner,
                )
                .await
            {
                Ok(_) => false,
                Err(e) => {
                    tracing::warn!(error = %e, "S3 DeleteObjects 批量删除失败，逐键回退");
                    true
                }
            }
        };

        for (i, k) in keys.iter().enumerate() {
            if let Some(msg) = &per_key_err[i] {
                let (code, text) = if msg == "PreconditionFailed" {
                    (
                        "PreconditionFailed".to_string(),
                        "a precondition on this object failed".to_string(),
                    )
                } else {
                    ("InvalidArgument".to_string(), msg.clone())
                };
                errors.push(S3DeleteError {
                    key: Some(k.clone()),
                    code: Some(code),
                    message: Some(text),
                    ..Default::default()
                });
                continue;
            }
            if batch_failed {
                // 逐键回退（NotFound 幂等 = 记 deleted）
                let p = match norm(k) {
                    Ok(p) => p,
                    Err(e) => {
                        errors.push(S3DeleteError {
                            key: Some(k.clone()),
                            code: Some("InvalidArgument".to_string()),
                            message: Some(e.to_string()),
                            ..Default::default()
                        });
                        continue;
                    }
                };
                match self
                    .workspace
                    .delete_entries(
                        &self.namespace,
                        std::slice::from_ref(&p),
                        Some("S3 DeleteObjects"),
                        &self.owner,
                    )
                    .await
                {
                    Ok(_) | Err(vfiles_domain::DomainError::NotFound { .. }) => {}
                    Err(e) => {
                        errors.push(S3DeleteError {
                            key: Some(k.clone()),
                            code: Some("InternalError".to_string()),
                            message: Some(e.to_string()),
                            ..Default::default()
                        });
                        continue;
                    }
                }
            }
            if !quiet {
                deleted.push(DeletedObject {
                    key: Some(k.clone()),
                    ..Default::default()
                });
            }
        }
        let out = DeleteObjectsOutput {
            deleted: (!deleted.is_empty()).then_some(deleted),
            errors: (!errors.is_empty()).then_some(errors),
            ..Default::default()
        };
        ok(out)
    }

    async fn delete_object(
        &self,
        req: S3Request<DeleteObjectInput>,
    ) -> S3Result<S3Response<DeleteObjectOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let path = norm(&input.key).map_err(dom_err)?;
        // `versionId` 定向删（非最新版 → 删该行；最新版需删除标记 ✗ 本实现诚实拒绝）
        if let Some(vid) = input.version_id.clone() {
            let entry = self
                .entry_at(&path)
                .await?
                .ok_or_else(|| s3s::s3_error!(NoSuchKey, "No such key"))?;
            let version_id = vfiles_domain::VersionId::from_string(&vid)
                .map_err(|_| s3s::s3_error!(NoSuchVersion, "no such version"))?;
            let ev = self
                .entry_repo
                .find_version(&version_id)
                .await
                .map_err(|_| s3s::s3_error!(NoSuchVersion, "no such version"))?;
            if ev.entry_id != entry.id {
                return Err(s3s::s3_error!(
                    NoSuchVersion,
                    "version does not belong to this key"
                ));
            }
            if entry.current_version_id == Some(ev.id) {
                return Err(s3s::s3_error!(
                    InvalidRequest,
                    "deleting the current version requires delete markers, which are not implemented"
                ));
            }
            if !self
                .entry_repo
                .delete_version(&version_id)
                .await
                .map_err(dom_err)?
            {
                return Err(s3s::s3_error!(NoSuchVersion, "no such version"));
            }
            return ok(DeleteObjectOutput::default());
        }
        // 条件删（`If-Match` ✗ 不存在/不符即 412）
        let cur = self.etag_at(&path).await?;
        check_dest_conditions(cur.as_deref(), input.if_match.as_ref(), None)?;
        match self
            .workspace
            .delete_entries(
                &self.namespace,
                std::slice::from_ref(&path),
                Some("S3 DELETE"),
                &self.owner,
            )
            .await
        {
            // S3 DELETE 幂等语义：不存在也 204 ✓
            Ok(_) | Err(vfiles_domain::DomainError::NotFound { .. }) => {
                ok(DeleteObjectOutput::default())
            }
            Err(e) => Err(dom_err(e)),
        }
    }

    /// multipart 开启：建未知大小的上传会话，返回 uploadId（= 上传会话 id）。
    async fn create_multipart_upload(
        &self,
        req: S3Request<CreateMultipartUploadInput>,
    ) -> S3Result<S3Response<CreateMultipartUploadOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let path = norm(&input.key).map_err(dom_err)?;
        let (parent_str, filename) = split_key(path.as_str());
        let parent = vfiles_domain::NormalizedPath::new(&parent_str)
            .map_err(|e| s3s::s3_error!(InvalidArgument, "{}", e))?;
        let view = self
            .upload
            .init_multipart_upload(
                &self.namespace,
                &parent,
                &filename,
                input.content_type.as_deref(),
                &self.owner,
            )
            .await
            .map_err(dom_err)?;
        // `x-amz-meta-*` 存会话（条目此时尚不存在 ✗ 完成时落到 entry 属性）
        if let Some(md) = &input.metadata
            && !md.is_empty()
        {
            let map: std::collections::BTreeMap<String, String> =
                md.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            self.upload
                .set_upload_custom_metadata(&view.upload_id, &map)
                .await
                .map_err(dom_err)?;
        }
        let out = CreateMultipartUploadOutput {
            bucket: Some(input.bucket),
            key: Some(input.key),
            upload_id: Some(view.upload_id.to_string()),
            ..Default::default()
        };
        ok(out)
    }

    /// 上传单个 part（partNumber 1..=10000 ✗ 内部索引 = partNumber-1，零基连续）。
    async fn upload_part(
        &self,
        req: S3Request<UploadPartInput>,
    ) -> S3Result<S3Response<UploadPartOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        if !(1..=10_000).contains(&input.part_number) {
            return Err(s3s::s3_error!(
                InvalidArgument,
                "part number must be between 1 and 10000"
            ));
        }
        let upload_id = parse_upload_id(&input.upload_id)?;
        self.validate_multipart_target(&upload_id, &input.key)
            .await?;
        let expected_md5 = decode_content_md5(input.content_md5.as_deref())?;
        let expected_sha256 = decode_checksum_sha256(input.checksum_sha256.as_deref())?;
        let expected_crc32 = decode_checksum_crc32(input.checksum_crc32.as_deref())?;
        let expected_crc32c = decode_checksum_crc32c(input.checksum_crc32c.as_deref())?;
        let expected_crc64nvme = decode_checksum_crc64nvme(input.checksum_crc64nvme.as_deref())?;
        let response_checksum_sha256 = input.checksum_sha256.clone();
        let response_checksum_crc32 = input.checksum_crc32.clone();
        let response_checksum_crc32c = input.checksum_crc32c.clone();
        let response_checksum_crc64nvme = input.checksum_crc64nvme.clone();
        let blob = input
            .body
            .unwrap_or_else(|| StreamingBlob::from_bytes(Default::default()));
        let receipt = self
            .upload
            .upload_part_from_stream(
                &upload_id,
                (input.part_number - 1) as u32,
                None,
                Some(5 * 1024 * 1024 * 1024),
                expected_md5,
                expected_sha256,
                expected_crc32,
                expected_crc32c,
                expected_crc64nvme,
                Box::new(stream_reader(blob)),
            )
            .await
            .map_err(|error| match error {
                vfiles_domain::DomainError::UploadPartChecksumMismatch => {
                    s3s::s3_error!(BadDigest, "uploaded part checksum did not match")
                }
                other => dom_err(other),
            })?;
        let out = UploadPartOutput {
            e_tag: Some(s3s::dto::ETag::Strong(receipt.md5_hex)),
            checksum_sha256: response_checksum_sha256,
            checksum_crc32: response_checksum_crc32,
            checksum_crc32c: response_checksum_crc32c,
            checksum_crc64nvme: response_checksum_crc64nvme,
            ..Default::default()
        };
        ok(out)
    }

    /// 分片复制（大对象服务端拷贝路径 ✗ `aws s3api upload-part-copy` / rclone 大文件复制）。
    ///
    /// `copy_source` = `<bucket>/<key>`；`copy_source_range` = `bytes=start-end`（闭区间 ✗ 单边可省）。
    async fn upload_part_copy(
        &self,
        req: S3Request<UploadPartCopyInput>,
    ) -> S3Result<S3Response<UploadPartCopyOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        if !(1..=10_000).contains(&input.part_number) {
            return Err(s3s::s3_error!(
                InvalidArgument,
                "part number must be between 1 and 10000"
            ));
        }
        let upload_id = parse_upload_id(&input.upload_id)?;
        self.validate_multipart_target(&upload_id, &input.key)
            .await?;
        let (src_bucket, src_key) = match &input.copy_source {
            s3s::dto::CopySource::Bucket { bucket, key, .. } => {
                (bucket.to_string(), key.to_string())
            }
            _ => {
                return Err(s3s::s3_error!(
                    InvalidArgument,
                    "only <bucket>/<key> copy sources are supported"
                ));
            }
        };
        if src_bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "source bucket not found"));
        }
        let src_path = norm(&src_key).map_err(dom_err)?;
        let (src_etag, src_mtime) = source_identity(self, &src_path).await?;
        check_copy_conditions(
            input.copy_source_if_match.as_ref(),
            input.copy_source_if_none_match.as_ref(),
            input.copy_source_if_modified_since.as_ref(),
            input.copy_source_if_unmodified_since.as_ref(),
            &src_etag,
            &src_mtime,
        )?;
        let mut content = self
            .workspace
            .open_file(&self.namespace, &src_path, None)
            .await
            .map_err(dom_err)?;
        let range = match input.copy_source_range.as_deref() {
            Some(spec) => copy_range_bounds(spec, content.size_bytes)?,
            None => 0..content.size_bytes,
        };
        let part_size = range.end - range.start;
        if part_size > MAX_S3_PART_SIZE {
            return Err(s3s::s3_error!(
                EntityTooLarge,
                "copied part exceeds the 5 GiB S3 limit"
            ));
        }
        use tokio::io::{AsyncSeekExt, SeekFrom};
        content
            .reader
            .seek(SeekFrom::Start(range.start))
            .await
            .map_err(|e| s3s::s3_error!(InternalError, "seek copy source: {}", e))?;
        let reader = content.reader.take(part_size);
        let receipt = self
            .upload
            .upload_part_from_stream(
                &upload_id,
                (input.part_number - 1) as u32,
                Some(part_size),
                Some(MAX_S3_PART_SIZE),
                None,
                None,
                None,
                None,
                None,
                Box::new(reader),
            )
            .await
            .map_err(dom_err)?;
        let out = UploadPartCopyOutput {
            copy_part_result: Some(CopyPartResult {
                e_tag: Some(s3s::dto::ETag::Strong(receipt.md5_hex)),
                last_modified: Some(Timestamp::from(time::OffsetDateTime::now_utc())),
                ..Default::default()
            }),
            ..Default::default()
        };
        ok(out)
    }

    /// 完成：校验列出 part 均已上传 → 拼接 → 落库（跳过量校验 ✗ 总大小未知）。
    async fn complete_multipart_upload(
        &self,
        req: S3Request<CompleteMultipartUploadInput>,
    ) -> S3Result<S3Response<CompleteMultipartUploadOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let upload_id = parse_upload_id(&input.upload_id)?;
        self.validate_multipart_target(&upload_id, &input.key)
            .await?;
        let stored = self
            .upload
            .list_upload_parts(&upload_id)
            .await
            .map_err(dom_err)?;
        if stored.is_empty() {
            return Err(s3s::s3_error!(InvalidPart, "no parts uploaded"));
        }
        let mpu = input
            .multipart_upload
            .as_ref()
            .ok_or_else(|| s3s::s3_error!(InvalidPart, "completed part list is required"))?;
        let mut listed = Vec::with_capacity(mpu.parts.iter().flatten().count());
        for part in mpu.parts.iter().flatten() {
            let number = part
                .part_number
                .ok_or_else(|| s3s::s3_error!(InvalidPart, "part number is required"))?;
            let etag = part
                .e_tag
                .as_ref()
                .ok_or_else(|| s3s::s3_error!(InvalidPart, "part ETag is required"))?;
            listed.push((number, etag.value().to_string()));
        }
        if listed.is_empty() {
            return Err(s3s::s3_error!(InvalidPart, "completed part list is empty"));
        }
        // S3 要求清单按 partNumber 严格递增；比较序列而非集合，拒绝重复号和乱序清单。
        if listed.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err(s3s::s3_error!(
                InvalidPart,
                "parts must be in ascending order"
            ));
        }
        let expected: Vec<i32> = stored.iter().map(|p| p.part_index as i32 + 1).collect();
        if listed.iter().map(|(number, _)| *number).collect::<Vec<_>>() != expected {
            return Err(s3s::s3_error!(
                InvalidPart,
                "the listed parts do not match the uploaded parts"
            ));
        }
        // 客户端必须回显每个 UploadPart 返回的 ETag，且值需与已存分片内容一致。
        for (number, etag) in listed {
            match self
                .upload
                .read_upload_part(&upload_id, (number - 1) as u32)
                .await
                .map_err(dom_err)?
            {
                Some(bytes) if md5_hex(&bytes) == etag => {}
                _ => {
                    return Err(s3s::s3_error!(InvalidPart, "part {} etag mismatch", number));
                }
            }
        }
        // 会话上存的 `x-amz-meta-*` 必须在完成**之前**读（完成会清掉会话目录）
        let custom = self
            .upload
            .get_upload_custom_metadata(&upload_id)
            .await
            .unwrap_or_default();
        let result = self
            .upload
            .complete_multipart_upload(&upload_id, Some("S3 multipart"))
            .await
            .map_err(dom_err)?;
        let etag = result.version.id.to_string().replace('-', "");
        // 会话上存的 `x-amz-meta-*` → 条目属性（完成即定稿 = 覆盖写语义）
        let meta_path = norm(&input.key).map_err(dom_err)?;
        if let Some(entry) = self.entry_at(&meta_path).await? {
            let map: s3s::dto::Metadata = custom.into_iter().collect();
            self.store_metadata(&entry.id, (!map.is_empty()).then_some(&map))
                .await?;
        }
        let location = format!("/{}/{}", input.bucket, input.key);
        let out = CompleteMultipartUploadOutput {
            bucket: Some(input.bucket),
            key: Some(input.key),
            e_tag: Some(s3s::dto::ETag::Strong(etag)),
            location: Some(location),
            ..Default::default()
        };
        ok(out)
    }

    /// 中止：取消上传会话（幂等 ✗ 已取消/过期按成功处理由 app 层决定）。
    async fn abort_multipart_upload(
        &self,
        req: S3Request<AbortMultipartUploadInput>,
    ) -> S3Result<S3Response<AbortMultipartUploadOutput>> {
        let input = req.input;
        self.require_write(req.credentials.as_ref())?;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let upload_id = parse_upload_id(&input.upload_id)?;
        self.validate_multipart_target(&upload_id, &input.key)
            .await?;
        match self.upload.cancel_upload(&upload_id).await {
            Ok(()) | Err(vfiles_domain::DomainError::NotFound { .. }) => {
                ok(AbortMultipartUploadOutput::default())
            }
            Err(e) => Err(dom_err(e)),
        }
    }

    /// 列出已上传 part（支持 part-number-marker / max-parts 分页）。
    async fn list_parts(
        &self,
        req: S3Request<ListPartsInput>,
    ) -> S3Result<S3Response<ListPartsOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let upload_id = parse_upload_id(&input.upload_id)?;
        self.validate_multipart_target(&upload_id, &input.key)
            .await?;
        let stored = self
            .upload
            .list_upload_parts(&upload_id)
            .await
            .map_err(dom_err)?;
        let marker = input.part_number_marker.unwrap_or(0);
        if !(0..=10_000).contains(&marker) {
            return Err(s3s::s3_error!(
                InvalidArgument,
                "part-number-marker must be between 0 and 10000"
            ));
        }
        let max_parts = input.max_parts.unwrap_or(1000);
        if !(1..=1000).contains(&max_parts) {
            return Err(s3s::s3_error!(
                InvalidArgument,
                "max-parts must be between 1 and 1000"
            ));
        }
        let page: Vec<_> = stored
            .iter()
            .filter(|part| part.part_index as i64 + 1 > marker as i64)
            .take(max_parts as usize + 1)
            .collect();
        let is_truncated = page.len() > max_parts as usize;
        let next_marker = is_truncated.then(|| page[max_parts as usize - 1].part_index as i32 + 1);
        let mut parts: Vec<Part> = Vec::with_capacity(page.len().min(max_parts as usize));
        for p in page.into_iter().take(max_parts as usize) {
            let bytes = self
                .upload
                .read_upload_part(&upload_id, p.part_index)
                .await
                .map_err(dom_err)?
                .ok_or_else(|| s3s::s3_error!(InternalError, "uploaded part content is missing"))?;
            parts.push(Part {
                part_number: Some(p.part_index as i32 + 1),
                size: Some(p.size_bytes.as_u64() as i64),
                last_modified: Some(Timestamp::from(p.received_at)),
                e_tag: Some(s3s::dto::ETag::Strong(md5_hex(&bytes))),
                ..Default::default()
            });
        }
        let out = ListPartsOutput {
            bucket: Some(input.bucket),
            key: Some(input.key),
            upload_id: Some(input.upload_id),
            parts: Some(parts),
            max_parts: Some(max_parts),
            part_number_marker: Some(marker),
            next_part_number_marker: next_marker,
            is_truncated: Some(is_truncated),
            ..Default::default()
        };
        ok(out)
    }

    /// 列出进行中的分片上传（`aws s3api list-multipart-uploads` / rclone 清理路径）。
    ///
    /// 支持 `prefix` / `delimiter` / `max-uploads` / `key-marker` + `upload-id-marker` 续页。
    async fn list_multipart_uploads(
        &self,
        req: S3Request<ListMultipartUploadsInput>,
    ) -> S3Result<S3Response<ListMultipartUploadsOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let prefix = input.prefix.clone().unwrap_or_default();
        let max = input.max_uploads.unwrap_or(1000).clamp(1, 1000) as usize;
        let delim = input.delimiter.clone();
        let items = self
            .upload
            .list_upload_sessions_page(
                &self.namespace,
                &prefix,
                delim.as_deref(),
                input.key_marker.as_deref(),
                input.upload_id_marker.as_deref(),
                (max + 1) as u32,
            )
            .await
            .map_err(dom_err)?;
        let mut combined: Vec<(String, Option<MultipartUpload>)> = items
            .into_iter()
            .map(|item| match item {
                vfiles_domain::UploadSessionListItem::CommonPrefix(prefix) => (prefix, None),
                vfiles_domain::UploadSessionListItem::Upload(session) => {
                    let key = if session.target_path_norm.as_str().is_empty() {
                        session.filename.clone()
                    } else {
                        format!("{}/{}", session.target_path_norm.as_str(), session.filename)
                    };
                    (
                        key.clone(),
                        Some(MultipartUpload {
                            key: Some(key),
                            upload_id: Some(session.id.to_string()),
                            initiated: Some(Timestamp::from(session.created_at)),
                            ..Default::default()
                        }),
                    )
                }
            })
            .collect();
        let truncated = combined.len() > max;
        combined.truncate(max);
        let last = combined.last();
        let next_key = last.map(|(k, _)| k.clone());
        let next_uid = last.and_then(|(_, u)| u.as_ref().and_then(|u| u.upload_id.clone()));
        let uploads: Vec<MultipartUpload> =
            combined.iter().filter_map(|(_, u)| u.clone()).collect();
        let cps: Vec<CommonPrefix> = combined
            .iter()
            .filter(|(_, u)| u.is_none())
            .map(|(c, _)| CommonPrefix {
                prefix: Some(c.clone()),
            })
            .collect();
        let out = ListMultipartUploadsOutput {
            bucket: Some(input.bucket),
            delimiter: input.delimiter,
            is_truncated: Some(truncated),
            key_marker: input.key_marker,
            max_uploads: Some(max as i32),
            prefix: input.prefix,
            upload_id_marker: input.upload_id_marker,
            next_key_marker: if truncated { next_key } else { None },
            next_upload_id_marker: if truncated { next_uid } else { None },
            uploads: Some(uploads),
            common_prefixes: (!cps.is_empty()).then_some(cps),
            ..Default::default()
        };
        ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(key: &str, size: u64) -> ObjMeta {
        ObjMeta {
            key: key.to_string(),
            size,
            last_modified: Timestamp::default(),
            etag: "00".repeat(16),
        }
    }

    fn keys(entries: &[Listed]) -> Vec<String> {
        entries.iter().map(|e| e.key().to_string()).collect()
    }

    fn sample() -> Vec<ObjMeta> {
        vec![
            meta("a.txt", 1),
            meta("b.txt", 2),
            meta("dir/x", 3),
            meta("dir/y", 4),
            meta("dir/sub/z", 5),
        ]
    }

    /// `x-amz-copy-source-if-*` 四头条件门控（ETag 匹配/不匹配 + 时间上下界）。
    #[test]
    fn copy_conditions_gate() {
        use s3s::dto::ETag;
        let etag = "abc123";
        let now = Timestamp::from(time::OffsetDateTime::now_utc());
        let past = Timestamp::from(time::OffsetDateTime::now_utc() - time::Duration::seconds(60));
        let future = Timestamp::from(time::OffsetDateTime::now_utc() + time::Duration::seconds(60));
        let hit = ETagCondition::ETag(ETag::Strong("abc123".to_string()));
        let miss = ETagCondition::ETag(ETag::Strong("other".to_string()));
        let any = ETagCondition::Any;
        let ok = |a, b, c, d| check_copy_conditions(a, b, c, d, etag, &now).is_ok();
        assert!(ok(Some(&hit), None, None, None), "if-match 命中");
        assert!(!ok(Some(&miss), None, None, None), "if-match 不命中");
        assert!(ok(Some(&any), None, None, None), "if-match * 恒真");
        assert!(!ok(None, Some(&hit), None, None), "if-none-match 命中即拒");
        assert!(
            ok(None, Some(&miss), None, None),
            "if-none-match 不命中放行"
        );
        assert!(!ok(None, Some(&any), None, None), "if-none-match * 恒拒");
        assert!(ok(None, None, Some(&past), None), "modified-since 已修改");
        assert!(
            !ok(None, None, Some(&future), None),
            "modified-since 未修改即拒"
        );
        assert!(
            ok(None, None, None, Some(&future)),
            "unmodified-since 未修改"
        );
        assert!(
            !ok(None, None, None, Some(&past)),
            "unmodified-since 已修改即拒"
        );
    }

    /// 流式分页（SQL 逐页）与参考实现（全量折叠 + 切片）**逐页走全等价**。
    #[test]
    fn streaming_pagination_walks_all_without_gaps() {
        fn stream(
            all: &[ObjMeta],
            prefix: &str,
            delim: Option<&str>,
            after: Option<&str>,
            max: usize,
        ) -> (Vec<String>, bool) {
            let mut c = ListCollector::new(prefix, delim, after, max);
            for o in all {
                if !c.push(o.clone()) {
                    break;
                }
            }
            let (e, t) = c.finish();
            (e.iter().map(|x| x.key().to_string()).collect(), t)
        }

        let mut all: Vec<ObjMeta> = Vec::new();
        for i in 0..37u64 {
            all.push(meta(&format!("d{}/f{:02}.txt", i % 4, i), i));
        }
        // `d0/a/x` 造更深一层（`d0/` 折叠不变）；`d4` 与 `zz`
        all.push(meta("d0/a/x", 1));
        all.push(meta("d4", 1));
        all.push(meta("zz", 1));
        all.sort_by(|a, b| a.key.cmp(&b.key));
        all.dedup_by(|a, b| a.key == b.key);

        for prefix in ["", "a", "d0/", "d1/", "zz", "d"] {
            for delim in [None, Some("/")] {
                let reference: Vec<String> = build_entries(&all, prefix, delim)
                    .iter()
                    .map(|e| e.key().to_string())
                    .collect();
                let mut seen: Vec<String> = Vec::new();
                let mut cursor: Option<String> = None;
                for _ in 0..500 {
                    let (keys, truncated) = stream(&all, prefix, delim, cursor.as_deref(), 3);
                    for k in &keys {
                        assert!(!seen.contains(k), "重复 key: {k}");
                    }
                    let last = keys.last().cloned();
                    seen.extend(keys);
                    if !truncated {
                        break;
                    }
                    cursor = last;
                }
                assert_eq!(
                    seen, reference,
                    "prefix={prefix:?} delim={delim:?} 分页走全应与参考一致"
                );
            }
        }
    }

    /// `x-amz-copy-source-range` 解析（闭区间 / 开尾 / 后缀 / 越界）。
    #[test]
    fn copy_range_slices() {
        let d = b"0123456789";
        assert_eq!(slice_copy_range(d, "bytes=0-3").unwrap(), b"0123");
        assert_eq!(slice_copy_range(d, "bytes=5-").unwrap(), b"56789");
        assert_eq!(slice_copy_range(d, "bytes=-3").unwrap(), b"789");
        assert_eq!(
            slice_copy_range(d, "bytes=8-99").unwrap(),
            b"89",
            "尾越界夹取"
        );
        assert!(slice_copy_range(d, "bytes=10-12").is_err(), "起点越界");
        assert!(slice_copy_range(d, "bytes=5-2").is_err(), "逆序");
        assert!(slice_copy_range(d, "0-3").is_err(), "缺 bytes= 前缀");
        assert!(slice_copy_range(b"", "bytes=0-0").is_err(), "空源");
    }

    /// delimiter 折叠 = 只折一层（AWS 语义）✗ 目录本身不产出对象。
    #[test]
    fn delimiter_rolls_up_one_level() {
        let all = sample();
        let e = build_entries(&all, "", Some("/"));
        assert_eq!(keys(&e), vec!["a.txt", "b.txt", "dir/"]);
        // 带 prefix 再折一层
        let e2 = build_entries(&all, "dir/", Some("/"));
        assert_eq!(keys(&e2), vec!["dir/sub/", "dir/x", "dir/y"]);
        // 无 delimiter = 全平铺
        let e3 = build_entries(&all, "", None);
        assert_eq!(
            keys(&e3),
            vec!["a.txt", "b.txt", "dir/sub/z", "dir/x", "dir/y"]
        );
    }

    /// prefix 过滤 + common prefix 去重计数（CommonPrefixes 各算一项）。
    #[test]
    fn prefix_filter_and_dedup() {
        let all = sample();
        let e = build_entries(&all, "dir", Some("/"));
        // "dir/x"、"dir/y" 折入 "dir/"；"dir/sub/z" 首分隔符也在 dir/ 后 → 同折
        assert_eq!(keys(&e), vec!["dir/"]);
        let e2 = build_entries(&all, "a", None);
        assert_eq!(keys(&e2), vec!["a.txt"]);
    }

    /// 分页：截断 + next = 页尾 key；再以 next 为 after 得下一页（不重不漏）。
    #[test]
    fn paginate_resumes_without_gap_or_duplicate() {
        let all = sample();
        let entries = build_entries(&all, "", None);
        let p1 = paginate(entries.clone(), None, 2);
        assert_eq!(
            p1.contents
                .iter()
                .map(|o| o.key.clone().unwrap())
                .collect::<Vec<_>>(),
            vec!["a.txt", "b.txt"]
        );
        assert!(p1.truncated);
        assert_eq!(p1.next.as_deref(), Some("b.txt"));
        let p2 = paginate(entries, p1.next.as_deref(), 2);
        assert_eq!(
            p2.contents
                .iter()
                .map(|o| o.key.clone().unwrap())
                .collect::<Vec<_>>(),
            vec!["dir/sub/z", "dir/x"]
        );
        assert!(p2.truncated);
        assert_eq!(p2.next.as_deref(), Some("dir/x"));
    }

    /// 满页且无余量 = 不截断（边界：rest.len() == max）。
    #[test]
    fn paginate_exact_fit_not_truncated() {
        let all = vec![meta("a", 1), meta("b", 1)];
        let entries = build_entries(&all, "", None);
        let p = paginate(entries, None, 2);
        assert!(!p.truncated);
        assert_eq!(p.next, None);
    }

    #[test]
    fn max_keys_rules() {
        assert_eq!(resolve_max_keys(None).unwrap(), 1000);
        assert_eq!(resolve_max_keys(Some(0)).unwrap(), 0);
        assert_eq!(resolve_max_keys(Some(5000)).unwrap(), 1000);
        assert!(
            resolve_max_keys(Some(-1)).is_err(),
            "负值 = InvalidArgument"
        );
    }

    #[test]
    fn range_resolution_matches_http_semantics() {
        let size = 100u64;
        let int = s3s::dto::Range::parse("bytes=0-9").unwrap();
        assert_eq!(resolve_range(Some(int), size).unwrap(), Some(0..10));
        let open = s3s::dto::Range::parse("bytes=90-").unwrap();
        assert_eq!(resolve_range(Some(open), size).unwrap(), Some(90..100));
        let suffix = s3s::dto::Range::parse("bytes=-10").unwrap();
        assert_eq!(resolve_range(Some(suffix), size).unwrap(), Some(90..100));
        let past = s3s::dto::Range::parse("bytes=200-").unwrap();
        assert!(resolve_range(Some(past), size).is_err(), "越界 = 416");
        assert_eq!(resolve_range(None, size).unwrap(), None);
    }
}

/// 按凭证分发到不同命名空间的服务实例（S3 凭证→命名空间绑定 ✗ 未绑定者走 default）。
#[derive(Debug)]
pub struct S3Router {
    pub default_service: Box<VfilesS3>,
    pub by_key: std::collections::HashMap<String, Box<VfilesS3>>,
    /// Explicit namespace bindings that failed to resolve must fail closed.
    pub rejected_keys: std::collections::HashSet<String>,
    /// 期望签名区域（空 = 不校验 ✗ 设了比对 `Authorization` Credential scope 的 region 段）。
    pub expected_region: String,
}

impl S3Router {
    /// 按凭证选命名空间实例 + （可选）区域校验（单点 ✗ 覆盖全部委托）。
    fn pick<T>(&self, req: &S3Request<T>) -> S3Result<&VfilesS3> {
        if !self.expected_region.is_empty()
            && let Some(az) = req.headers.get(axum::http::header::AUTHORIZATION)
            && let Ok(auth) = az.to_str()
        {
            // Credential=<AK>/<date>/<region>/<service>/<aws4_request>
            let region = auth.split('/').nth(2).unwrap_or_default();
            if region != self.expected_region {
                let msg = format!(
                    "region '{}' is wrong; expecting '{}'",
                    region, self.expected_region
                );
                return Err(s3s::s3_error!(AuthorizationHeaderMalformed, "{}", msg));
            }
        }
        let creds = req.credentials.as_ref();
        if creds.is_some_and(|c| self.rejected_keys.contains(&c.access_key)) {
            return Err(s3s::s3_error!(
                AccessDenied,
                "the access key is bound to an unavailable namespace"
            ));
        }
        Ok(creds
            .and_then(|c| self.by_key.get(&c.access_key))
            .map(|b| b.as_ref())
            .unwrap_or(self.default_service.as_ref()))
    }
}

#[async_trait]
impl S3 for S3Router {
    async fn list_object_versions(
        &self,
        req: S3Request<ListObjectVersionsInput>,
    ) -> S3Result<S3Response<ListObjectVersionsOutput>> {
        self.pick(&req)?.list_object_versions(req).await
    }

    async fn put_bucket_versioning(
        &self,
        req: S3Request<PutBucketVersioningInput>,
    ) -> S3Result<S3Response<PutBucketVersioningOutput>> {
        self.pick(&req)?.put_bucket_versioning(req).await
    }

    /// 桶清单对所有命名空间同形（唯一虚拟桶 ✗ `_req` 形故上面正则未捕获，手写委托）。
    async fn list_buckets(
        &self,
        req: S3Request<ListBucketsInput>,
    ) -> S3Result<S3Response<ListBucketsOutput>> {
        self.pick(&req)?.list_buckets(req).await
    }

    /// 桶生命周期（r34 后补的手写委托，与上面同因）。
    async fn create_bucket(
        &self,
        req: S3Request<CreateBucketInput>,
    ) -> S3Result<S3Response<CreateBucketOutput>> {
        self.pick(&req)?.create_bucket(req).await
    }

    async fn delete_bucket(
        &self,
        req: S3Request<DeleteBucketInput>,
    ) -> S3Result<S3Response<DeleteBucketOutput>> {
        self.pick(&req)?.delete_bucket(req).await
    }

    async fn list_objects_v2(
        &self,
        req: S3Request<ListObjectsV2Input>,
    ) -> S3Result<S3Response<ListObjectsV2Output>> {
        self.pick(&req)?.list_objects_v2(req).await
    }

    async fn head_bucket(
        &self,
        req: S3Request<HeadBucketInput>,
    ) -> S3Result<S3Response<HeadBucketOutput>> {
        self.pick(&req)?.head_bucket(req).await
    }

    async fn get_bucket_location(
        &self,
        req: S3Request<GetBucketLocationInput>,
    ) -> S3Result<S3Response<GetBucketLocationOutput>> {
        self.pick(&req)?.get_bucket_location(req).await
    }

    async fn get_bucket_versioning(
        &self,
        req: S3Request<GetBucketVersioningInput>,
    ) -> S3Result<S3Response<GetBucketVersioningOutput>> {
        self.pick(&req)?.get_bucket_versioning(req).await
    }

    async fn list_objects(
        &self,
        req: S3Request<ListObjectsInput>,
    ) -> S3Result<S3Response<ListObjectsOutput>> {
        self.pick(&req)?.list_objects(req).await
    }

    async fn get_object(
        &self,
        req: S3Request<GetObjectInput>,
    ) -> S3Result<S3Response<GetObjectOutput>> {
        self.pick(&req)?.get_object(req).await
    }

    async fn head_object(
        &self,
        req: S3Request<HeadObjectInput>,
    ) -> S3Result<S3Response<HeadObjectOutput>> {
        self.pick(&req)?.head_object(req).await
    }

    async fn put_object(
        &self,
        req: S3Request<PutObjectInput>,
    ) -> S3Result<S3Response<PutObjectOutput>> {
        self.pick(&req)?.put_object(req).await
    }

    async fn copy_object(
        &self,
        req: S3Request<CopyObjectInput>,
    ) -> S3Result<S3Response<CopyObjectOutput>> {
        self.pick(&req)?.copy_object(req).await
    }

    async fn delete_objects(
        &self,
        req: S3Request<DeleteObjectsInput>,
    ) -> S3Result<S3Response<DeleteObjectsOutput>> {
        self.pick(&req)?.delete_objects(req).await
    }

    async fn delete_object(
        &self,
        req: S3Request<DeleteObjectInput>,
    ) -> S3Result<S3Response<DeleteObjectOutput>> {
        self.pick(&req)?.delete_object(req).await
    }

    async fn create_multipart_upload(
        &self,
        req: S3Request<CreateMultipartUploadInput>,
    ) -> S3Result<S3Response<CreateMultipartUploadOutput>> {
        self.pick(&req)?.create_multipart_upload(req).await
    }

    async fn upload_part(
        &self,
        req: S3Request<UploadPartInput>,
    ) -> S3Result<S3Response<UploadPartOutput>> {
        self.pick(&req)?.upload_part(req).await
    }

    async fn upload_part_copy(
        &self,
        req: S3Request<UploadPartCopyInput>,
    ) -> S3Result<S3Response<UploadPartCopyOutput>> {
        self.pick(&req)?.upload_part_copy(req).await
    }

    async fn complete_multipart_upload(
        &self,
        req: S3Request<CompleteMultipartUploadInput>,
    ) -> S3Result<S3Response<CompleteMultipartUploadOutput>> {
        self.pick(&req)?.complete_multipart_upload(req).await
    }

    async fn abort_multipart_upload(
        &self,
        req: S3Request<AbortMultipartUploadInput>,
    ) -> S3Result<S3Response<AbortMultipartUploadOutput>> {
        self.pick(&req)?.abort_multipart_upload(req).await
    }

    async fn list_parts(
        &self,
        req: S3Request<ListPartsInput>,
    ) -> S3Result<S3Response<ListPartsOutput>> {
        self.pick(&req)?.list_parts(req).await
    }

    async fn list_multipart_uploads(
        &self,
        req: S3Request<ListMultipartUploadsInput>,
    ) -> S3Result<S3Response<ListMultipartUploadsOutput>> {
        self.pick(&req)?.list_multipart_uploads(req).await
    }
}
