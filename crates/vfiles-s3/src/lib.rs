//! S3 兼容 API（协议族并列 crate ✗ round r1 目标首件 · 近期路线第 1 件）。
//!
//! 形态：独立端口（S3 path-style 寻址与前端根语义冲突 ✗ MinIO 同款 9000 独立端口直觉 ✓）+
//! 单虚拟桶 `default`（严格语义：非 default 桶 → NoSuchBucket ✓ 客户端列桶自配对）+
//! 认证 = SigV4（s3s 内建 ✗ env 静态凭证表支持只读策略与命名空间 slug 绑定；未设时随机
//! 生成 + warn 打印 = 零配置试用）。
//!
//! 映射：key = 默认 ns 根下相对路径（tree 展开为 flat keys ✗ 单次 Put/Copy 使用内容 MD5
//! ETag，multipart 使用 AWS composite 形；两者均按版本持久化）。
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
use md5::{Digest, Md5};
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
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Hash an upload stream while it is persisted, so regular S3 PUT/COPY ETags don't require a
/// second read of the newly written object.
struct Md5Reader<R> {
    inner: R,
    digest: std::sync::Arc<std::sync::Mutex<Md5>>,
}

impl<R> Md5Reader<R> {
    fn new(inner: R) -> (Self, std::sync::Arc<std::sync::Mutex<Md5>>) {
        let digest = std::sync::Arc::new(std::sync::Mutex::new(Md5::new()));
        (
            Self {
                inner,
                digest: digest.clone(),
            },
            digest,
        )
    }
}

impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Md5Reader<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        match std::pin::Pin::new(&mut self.inner).poll_read(cx, buf) {
            std::task::Poll::Ready(Ok(())) => {
                let bytes = &buf.filled()[before..];
                if !bytes.is_empty() {
                    self.digest
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .update(bytes);
                }
                std::task::Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

fn finish_md5(digest: &std::sync::Mutex<Md5>) -> String {
    hex::encode(
        digest
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .finalize(),
    )
}

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
    pub delete_markers: vfiles_infra_sqlite::SqliteS3DeleteMarkerRepo,
    pub namespace: vfiles_domain::NamespaceId,
    pub owner: vfiles_domain::UserId,
    /// 只读凭证（`access:secret:ro` ✗ 变更类操作一律 AccessDenied）。
    pub readonly_keys: std::collections::HashSet<String>,
}

fn ok<T>(output: T) -> S3Result<S3Response<T>> {
    Ok(S3Response::new(output))
}

fn delete_marker_read_error() -> s3s::S3Error {
    let mut error = s3s::S3Error::with_message(
        s3s::S3ErrorCode::MethodNotAllowed,
        "the specified version is a delete marker",
    );
    let mut headers = http::HeaderMap::new();
    headers.insert(
        "x-amz-delete-marker",
        http::HeaderValue::from_static("true"),
    );
    error.set_headers(headers);
    error
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

fn obj_meta(
    m: vfiles_domain::types::EntryChildMeta,
    properties: &[(String, String)],
    version_created_at: Option<time::OffsetDateTime>,
) -> ObjMeta {
    let version_id = m.entry.current_version_id;
    let version_text = version_id.map(|version| version.to_string().replace('-', ""));
    let etag = version_text
        .as_deref()
        .and_then(|version| {
            let name = format!("{S3_ETAG_PREFIX}{version}");
            properties
                .iter()
                .find(|(property, _)| property == &name)
                .map(|(_, value)| value.clone())
        })
        .or(version_text)
        .unwrap_or_default();
    ObjMeta {
        key: m.entry.path_norm.as_str().to_string(),
        size: m.size_bytes.unwrap_or(0),
        last_modified: Timestamp::from(object_last_modified(
            m.entry.created_at,
            version_created_at,
        )),
        etag,
    }
}

fn object_last_modified(
    entry_created_at: time::OffsetDateTime,
    version_created_at: Option<time::OffsetDateTime>,
) -> time::OffsetDateTime {
    version_created_at.unwrap_or(entry_created_at)
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
        let ids: Vec<_> = batch.iter().map(|m| m.entry.id).collect();
        let properties = repo.list_entry_properties(&ids).await?;
        let version_ids: Vec<_> = batch
            .iter()
            .filter_map(|meta| meta.entry.current_version_id)
            .collect();
        let version_mtimes: std::collections::HashMap<_, _> = repo
            .find_versions(&version_ids)
            .await?
            .into_iter()
            .map(|version| (version.id, version.created_at))
            .collect();
        for m in batch {
            cursor = Some(m.entry.path_norm.as_str().to_string());
            let entry_properties = properties
                .get(&m.entry.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let version_created_at = m
                .entry
                .current_version_id
                .as_ref()
                .and_then(|version_id| version_mtimes.get(version_id).copied());
            if !c.push(obj_meta(m, entry_properties, version_created_at)) {
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

/// Read a bounded range from a seekable object as fixed-size chunks. The mutex makes the stream
/// `Sync`, which is required by s3s's response body stream, while the file itself remains single
/// reader and never gets copied into an object-sized allocation.
struct S3ReadStream {
    reader: std::sync::Mutex<Box<dyn vfiles_domain::ReadSeek + Send + Unpin>>,
    remaining: u64,
    buffer: Vec<u8>,
}

impl S3ReadStream {
    fn new(reader: Box<dyn vfiles_domain::ReadSeek + Send + Unpin>, length: u64) -> Self {
        Self {
            reader: std::sync::Mutex::new(reader),
            remaining: length,
            buffer: vec![0; 64 * 1024],
        }
    }
}

impl futures::Stream for S3ReadStream {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;
        let this = self.get_mut();
        if this.remaining == 0 {
            return Poll::Ready(None);
        }
        let cap = this.remaining.min(this.buffer.len() as u64) as usize;
        let read = {
            let mut reader = match this.reader.lock() {
                Ok(reader) => reader,
                Err(_) => {
                    return Poll::Ready(Some(Err(std::io::Error::other(
                        "S3 object reader mutex poisoned",
                    ))));
                }
            };
            let mut buf = tokio::io::ReadBuf::new(&mut this.buffer[..cap]);
            match tokio::io::AsyncRead::poll_read(std::pin::Pin::new(&mut **reader), cx, &mut buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
                Poll::Ready(Ok(())) => buf.filled().len(),
            }
        };
        if read == 0 {
            this.remaining = 0;
            return Poll::Ready(Some(Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "S3 object stream ended before its declared length",
            ))));
        }
        this.remaining -= read as u64;
        Poll::Ready(Some(Ok(bytes::Bytes::copy_from_slice(
            &this.buffer[..read],
        ))))
    }
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
    if backend.has_current_delete_marker(src_path).await? {
        return Err(s3s::s3_error!(NoSuchKey, "source key is deleted"));
    }
    let e = backend
        .entry_at(src_path)
        .await?
        .ok_or_else(|| s3s::s3_error!(NoSuchKey, "source key not found"))?;
    let version_id = e
        .current_version_id
        .ok_or_else(|| s3s::s3_error!(NoSuchKey, "source key has no current version"))?;
    let version = backend
        .entry_repo
        .find_version(&version_id)
        .await
        .map_err(dom_err)?;
    if version.entry_id != e.id {
        return Err(s3s::s3_error!(
            NoSuchKey,
            "source version does not match key"
        ));
    }
    let version_id_text = version_id.to_string().replace('-', "");
    let etags = backend.load_version_etags(&[e.id]).await?;
    let etag = etags
        .get(&e.id)
        .and_then(|map| map.get(&version_id_text))
        .cloned()
        .unwrap_or(version_id_text);
    Ok((etag, Timestamp::from(version.created_at)))
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

struct UploadPartChecksums {
    md5: [u8; 16],
    md5_hex: String,
    sha1: [u8; 20],
    sha256: [u8; 32],
    crc32: u32,
    crc32c: u32,
    crc64nvme: u64,
}

/// 一次流式读取计算 multipart ETag 与现有 S3 checksum，避免 Complete 将最大 5 GiB 分片
/// 聚合到内存。
async fn hash_upload_part(
    mut reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
) -> std::io::Result<UploadPartChecksums> {
    use md5::{Digest, Md5};
    use sha2::Digest as Sha2Digest;
    let mut md5 = Md5::new();
    let mut sha1 = sha1::Sha1::new();
    let mut sha256 = sha2::Sha256::new();
    let mut crc32 = crc32fast::Hasher::new();
    let mut crc32c = 0_u32;
    let crc64_spec = crc::Crc::<u64>::new(&crc::CRC_64_NVME);
    let mut crc64nvme = crc64_spec.digest();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        md5.update(&buf[..n]);
        sha1.update(&buf[..n]);
        Sha2Digest::update(&mut sha256, &buf[..n]);
        crc32.update(&buf[..n]);
        crc32c = crc32c::crc32c_append(crc32c, &buf[..n]);
        crc64nvme.update(&buf[..n]);
    }
    let md5: [u8; 16] = md5.finalize().into();
    let sha256: [u8; 32] = Sha2Digest::finalize(sha256).into();
    Ok(UploadPartChecksums {
        md5,
        md5_hex: hex::encode(md5),
        sha1: sha1.finalize().into(),
        sha256,
        crc32: crc32.finalize(),
        crc32c,
        crc64nvme: crc64nvme.finalize(),
    })
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

fn decode_checksum_sha1(value: Option<&str>) -> S3Result<Option<[u8; 20]>> {
    use base64::Engine;
    let Some(value) = value else { return Ok(None) };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| s3s::s3_error!(InvalidDigest, "x-amz-checksum-sha1 is not valid base64"))?;
    let bytes: [u8; 20] = decoded
        .try_into()
        .map_err(|_| s3s::s3_error!(InvalidDigest, "SHA1 checksum must decode to 20 bytes"))?;
    Ok(Some(bytes))
}

impl std::fmt::Debug for VfilesS3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // workspace/upload 等非 Debug ✗ 标准省内容式（-W missing-debug-implementations 清零）
        f.debug_struct("VfilesS3").finish_non_exhaustive()
    }
}

/// S3 用户元数据（`x-amz-meta-*`）在 entry 属性表中的前缀（复用 0006 `entry_properties`）。
const S3_META_PREFIX: &str = "s3-meta:";
/// Per-version ETag overrides for multipart objects.
const S3_ETAG_PREFIX: &str = "s3-etag:";

fn version_etag_property(version_id: &vfiles_domain::VersionId) -> String {
    format!(
        "{S3_ETAG_PREFIX}{}",
        version_id.to_string().replace('-', "")
    )
}

fn multipart_etag(part_md5s: &[[u8; 16]]) -> String {
    use md5::{Digest, Md5};
    let mut digest = Md5::new();
    for part_md5 in part_md5s {
        digest.update(part_md5);
    }
    format!("{}-{}", hex::encode(digest.finalize()), part_md5s.len())
}

fn resolve_completed_part_indices(stored: &[(i32, u64)], requested: &[i32]) -> S3Result<Vec<u32>> {
    if requested.is_empty() {
        return Err(s3s::s3_error!(InvalidPart, "completed part list is empty"));
    }
    if requested.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(s3s::s3_error!(
            InvalidPartOrder,
            "parts must be in ascending order"
        ));
    }
    let stored_by_number: std::collections::HashMap<i32, u64> = stored.iter().copied().collect();
    let mut indices = Vec::with_capacity(requested.len());
    for (position, number) in requested.iter().enumerate() {
        let Some(size) = stored_by_number.get(number).copied() else {
            return Err(s3s::s3_error!(
                InvalidPart,
                "part {} was not uploaded",
                number
            ));
        };
        if position + 1 != requested.len() && size < 5 * 1024 * 1024 {
            return Err(s3s::s3_error!(
                EntityTooSmall,
                "part {} is smaller than 5 MiB",
                number
            ));
        }
        indices.push((*number - 1) as u32);
    }
    Ok(indices)
}

impl VfilesS3 {
    async fn has_current_delete_marker(
        &self,
        path: &vfiles_domain::NormalizedPath,
    ) -> S3Result<bool> {
        let hidden = self
            .delete_markers
            .current_hidden_keys(&self.namespace, &[path.as_str().to_string()])
            .await
            .map_err(dom_err)?;
        Ok(hidden.contains(path.as_str()))
    }

    async fn filter_current_delete_markers(&self, page: &mut Page) -> S3Result<()> {
        let keys: Vec<_> = page
            .contents
            .iter()
            .filter_map(|object| object.key.clone())
            .collect();
        let hidden = self
            .delete_markers
            .current_hidden_keys(&self.namespace, &keys)
            .await
            .map_err(dom_err)?;
        page.contents
            .retain(|object| object.key.as_ref().is_none_or(|key| !hidden.contains(key)));
        Ok(())
    }

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

    async fn load_version_etags(
        &self,
        entry_ids: &[vfiles_domain::EntryId],
    ) -> S3Result<
        std::collections::HashMap<
            vfiles_domain::EntryId,
            std::collections::HashMap<String, String>,
        >,
    > {
        let properties = self
            .entry_repo
            .list_entry_properties(entry_ids)
            .await
            .map_err(dom_err)?;
        Ok(properties
            .into_iter()
            .map(|(entry_id, properties)| {
                let etags = properties
                    .into_iter()
                    .filter_map(|(name, value)| {
                        name.strip_prefix(S3_ETAG_PREFIX)
                            .map(|version_id| (version_id.to_owned(), value))
                    })
                    .collect();
                (entry_id, etags)
            })
            .collect())
    }

    async fn store_version_etag(
        &self,
        entry_id: &vfiles_domain::EntryId,
        version_id: &vfiles_domain::VersionId,
        etag: &str,
    ) -> S3Result<()> {
        self.entry_repo
            .set_entry_property(entry_id, &version_etag_property(version_id), etag)
            .await
            .map_err(dom_err)
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

    async fn permanently_delete_version(
        &self,
        path: &vfiles_domain::NormalizedPath,
        version_id: &str,
    ) -> S3Result<()> {
        let id = vfiles_domain::VersionId::from_string(version_id)
            .map_err(|_| s3s::s3_error!(NoSuchVersion, "no such version"))?;
        let entry = self
            .entry_at(path)
            .await?
            .ok_or_else(|| s3s::s3_error!(NoSuchVersion, "no such version"))?;
        let version = self
            .entry_repo
            .find_version(&id)
            .await
            .map_err(|_| s3s::s3_error!(NoSuchVersion, "no such version"))?;
        if version.entry_id != entry.id {
            return Err(s3s::s3_error!(NoSuchVersion, "no such version"));
        }
        let versions = self
            .entry_repo
            .find_versions_for_entries(&[entry.id])
            .await
            .map_err(dom_err)?;
        if versions.len() == 1 {
            self.workspace
                .delete_entries(
                    &self.namespace,
                    std::slice::from_ref(path),
                    Some("S3 delete object version"),
                    &self.owner,
                )
                .await
                .map_err(dom_err)?;
        } else {
            if !self.entry_repo.delete_version(&id).await.map_err(dom_err)? {
                return Err(s3s::s3_error!(NoSuchVersion, "no such version"));
            }
            self.entry_repo
                .remove_entry_property(&entry.id, &version_etag_property(&id))
                .await
                .map_err(dom_err)?;
        }
        Ok(())
    }

    /// 目标路径当前 ETag（按版本读出持久化值；历史版本回退到版本 id）。
    async fn etag_at(&self, path: &vfiles_domain::NormalizedPath) -> S3Result<Option<String>> {
        let Some(entry) = self.entry_at(path).await? else {
            return Ok(None);
        };
        let Some(version_id) = entry.current_version_id else {
            return Ok(None);
        };
        let version_id_text = version_id.to_string().replace('-', "");
        let etags = self.load_version_etags(&[entry.id]).await?;
        Ok(Some(
            etags
                .get(&entry.id)
                .and_then(|map| map.get(&version_id_text))
                .cloned()
                .unwrap_or(version_id_text),
        ))
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

    /// 目标版本（`versionId` 无 → 最新）：返回 `(etag, versionId, last_modified, commit)`。
    async fn resolve_version(
        &self,
        entry: &vfiles_domain::Entry,
        version_id: Option<&str>,
    ) -> S3Result<(String, String, Timestamp, Option<String>)> {
        let Some(vid) = version_id else {
            let Some(version_id) = entry.current_version_id else {
                return Ok((
                    String::new(),
                    String::new(),
                    Timestamp::from(entry.created_at),
                    None,
                ));
            };
            let version = self
                .entry_repo
                .find_version(&version_id)
                .await
                .map_err(dom_err)?;
            if version.entry_id != entry.id {
                return Err(s3s::s3_error!(
                    NoSuchKey,
                    "current version does not match key"
                ));
            }
            let version_id_text = version_id.to_string().replace('-', "");
            let etags = self.load_version_etags(&[entry.id]).await?;
            let etag = etags
                .get(&entry.id)
                .and_then(|map| map.get(&version_id_text))
                .cloned()
                .unwrap_or_else(|| version_id_text.clone());
            return Ok((
                etag,
                version_id_text,
                Timestamp::from(version.created_at),
                None,
            ));
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
        let version_id_text = ev.id.to_string().replace('-', "");
        let etags = self.load_version_etags(&[entry.id]).await?;
        let etag = etags
            .get(&entry.id)
            .and_then(|map| map.get(&version_id_text))
            .cloned()
            .unwrap_or_else(|| version_id_text.clone());
        Ok((
            etag,
            version_id_text,
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
        self.filter_current_delete_markers(&mut page).await?;
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

    /// 列出对象版本和删除标记（双表 key 游标分页，按 key 升序、版本时间倒序输出）。
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
        let mut out_markers: Vec<s3s::dto::DeleteMarkerEntry> = Vec::new();
        let mut prefixes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut truncated = false;
        let mut next_key: Option<String> = None;
        let mut next_vid: Option<String> = None;
        // 最后一次输出的 (key, version_id) ✗ 截断时作为续页游标
        let mut last_emitted: Option<(String, String)> = None;

        // The repository pages the union of file keys and delete-marker-only keys.
        // Version and marker rows are then fetched in batches per key page.
        let from = key_marker
            .as_deref()
            .filter(|marker| *marker > prefix.as_str())
            .unwrap_or(&prefix);
        let mut cursor: Option<String> = None;
        'outer: loop {
            let keys = self
                .delete_markers
                .keys_page(&self.namespace, &prefix, from, cursor.as_deref(), 256)
                .await
                .map_err(dom_err)?;
            if keys.is_empty() {
                break;
            }
            cursor = keys.last().cloned();
            let paths = keys
                .iter()
                .map(|key| norm(key).map_err(dom_err))
                .collect::<S3Result<Vec<_>>>()?;
            let entries = self
                .entry_repo
                .find_paths(&self.namespace, &paths)
                .await
                .map_err(dom_err)?;
            let entry_by_key: std::collections::HashMap<_, _> = entries
                .into_iter()
                .map(|entry| (entry.path_norm.as_str().to_string(), entry))
                .collect();
            let entry_ids: Vec<_> = entry_by_key.values().map(|entry| entry.id).collect();
            let version_orders = self
                .delete_markers
                .version_orders_for_entries(&entry_ids)
                .await
                .map_err(dom_err)?;
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
            let version_etags = self.load_version_etags(&entry_ids).await?;
            let markers = self
                .delete_markers
                .for_keys(&self.namespace, &keys)
                .await
                .map_err(dom_err)?;
            let mut markers_by_key: std::collections::HashMap<_, Vec<_>> =
                std::collections::HashMap::new();
            for marker in markers {
                markers_by_key
                    .entry(marker.object_key.clone())
                    .or_default()
                    .push(marker);
            }
            for key in keys {
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
                            if out_versions.len() + out_markers.len() + prefixes.len() >= max {
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
                let mut items: Vec<(
                    i64,
                    time::OffsetDateTime,
                    String,
                    Option<ObjectVersion>,
                    Option<s3s::dto::DeleteMarkerEntry>,
                )> = Vec::new();
                if let Some(entry) = entry_by_key.get(&key) {
                    let mut entry_versions =
                        versions_by_entry.remove(&entry.id).unwrap_or_default();
                    entry_versions.sort_by_key(|e| std::cmp::Reverse(e.version_no));
                    for ev in entry_versions {
                        let vid = ev.id.to_string().replace('-', "");
                        let etag = version_etags
                            .get(&entry.id)
                            .and_then(|map| map.get(&vid))
                            .cloned()
                            .unwrap_or_else(|| vid.clone());
                        items.push((
                            version_orders
                                .get(&ev.id.to_string())
                                .copied()
                                .unwrap_or_default(),
                            ev.created_at,
                            vid.clone(),
                            Some(ObjectVersion {
                                key: Some(key.clone()),
                                version_id: Some(vid),
                                size: Some(ev.size_bytes.as_u64() as i64),
                                last_modified: Some(Timestamp::from(ev.created_at)),
                                e_tag: Some(s3s::dto::ETag::Strong(etag)),
                                ..Default::default()
                            }),
                            None,
                        ));
                    }
                }
                for marker in markers_by_key.remove(&key).unwrap_or_default() {
                    items.push((
                        marker.event_order,
                        marker.created_at,
                        marker.version_id.clone(),
                        None,
                        Some(s3s::dto::DeleteMarkerEntry {
                            key: Some(key.clone()),
                            version_id: Some(marker.version_id),
                            last_modified: Some(Timestamp::from(marker.created_at)),
                            owner: Some(Owner {
                                id: Some(marker.owner_id),
                                display_name: None,
                            }),
                            ..Default::default()
                        }),
                    ));
                }
                items.sort_by(|a, b| {
                    b.0.cmp(&a.0)
                        .then_with(|| b.1.cmp(&a.1))
                        .then_with(|| b.2.cmp(&a.2))
                });
                for (index, (_, _, vid, mut version, mut marker)) in items.into_iter().enumerate() {
                    if let Some(version) = &mut version {
                        version.is_latest = Some(index == 0);
                    }
                    if let Some(marker) = &mut marker {
                        marker.is_latest = Some(index == 0);
                    }
                    if skipping {
                        if vid_marker.as_ref() == Some(&vid) {
                            skipping = false;
                        }
                        continue;
                    }
                    if out_versions.len() + out_markers.len() + prefixes.len() >= max {
                        truncated = true;
                        if let Some((k, v)) = &last_emitted {
                            next_key = Some(k.clone());
                            next_vid = Some(v.clone());
                        }
                        break 'outer;
                    }
                    if let Some(version) = version {
                        out_versions.push(version);
                    }
                    if let Some(marker) = marker {
                        out_markers.push(marker);
                    }
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
            for marker in &mut out_markers {
                if let Some(key) = &marker.key {
                    marker.key = Some(url_encode(key));
                }
                if let Some(version_id) = &marker.version_id {
                    marker.version_id = Some(url_encode(version_id));
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
            delete_markers: (!out_markers.is_empty()).then_some(out_markers),
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
        self.filter_current_delete_markers(&mut page).await?;
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
        if let Some(version_id) = input.version_id.as_deref()
            && self
                .delete_markers
                .contains_version(&self.namespace, path.as_str(), version_id)
                .await
                .map_err(dom_err)?
        {
            return Err(delete_marker_read_error());
        }
        if input.version_id.is_none() && self.has_current_delete_marker(&path).await? {
            return Err(s3s::s3_error!(NoSuchKey, "No such key"));
        }
        // 单次 Put/Copy 沿用 version id ETag；multipart 版本从 s3-etag 属性恢复 composite ETag。
        let entry = self
            .entry_repo
            .find_by_path(&self.namespace, &path)
            .await
            .map_err(dom_err)?
            .ok_or_else(|| s3s::s3_error!(NoSuchKey, "No such key"))?;
        // Strong 变体序列化自附引号 ✗ 存裸 hex 防双引（etag.rs:19-24）
        // `versionId` 定向：etag/时间取该版本 ✗ raw_commit 透传 = 正文/mime/size 同版本
        let (etag, version_id, last_modified, raw_commit) = self
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
        let (start, length, content_range) = match slice {
            Some(r) => (
                r.start,
                r.end - r.start,
                Some(format!("bytes {}-{}/{}", r.start, r.end - 1, size)),
            ),
            None => (0, size, None),
        };
        let mut reader = file.reader;
        if start > 0 {
            reader
                .seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|e| s3s::s3_error!(InternalError, "seek failed: {}", e))?;
        }
        let body = StreamingBlob::wrap(S3ReadStream::new(reader, length));
        let metadata = self.load_metadata(&entry.id).await?;
        let out = GetObjectOutput {
            body: Some(body),
            content_length: Some(length as i64),
            content_type: file.mime_type,
            accept_ranges: Some("bytes".to_string()),
            content_range,
            e_tag: Some(s3s::dto::ETag::Strong(etag.clone())),
            last_modified: Some(last_modified),
            metadata,
            // versionId 与 ETag 独立：multipart 的 ETag 不等于版本 ID。
            version_id: (!version_id.is_empty()).then_some(version_id),
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
        if let Some(version_id) = input.version_id.as_deref()
            && self
                .delete_markers
                .contains_version(&self.namespace, path.as_str(), version_id)
                .await
                .map_err(dom_err)?
        {
            return Err(delete_marker_read_error());
        }
        if input.version_id.is_none() && self.has_current_delete_marker(&path).await? {
            return Err(s3s::s3_error!(NoSuchKey, "No such key"));
        }
        let entry = self
            .entry_repo
            .find_by_path(&self.namespace, &path)
            .await
            .map_err(dom_err)?
            .ok_or_else(|| s3s::s3_error!(NoSuchKey, "No such key"))?;
        // `versionId` 定向：etag/时间取该版本 ✗ raw_commit 透传 = 正文/mime/size 同版本
        let (etag, version_id, last_modified, raw_commit) = self
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
            // versionId 与 ETag 独立：multipart 的 ETag 不等于版本 ID。
            version_id: (!version_id.is_empty()).then_some(version_id),
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
        let expected_sha1 = decode_checksum_sha1(input.checksum_sha1.as_deref())?;
        let expected_sha256_hex = expected_sha256.map(hex::encode);
        let response_checksum_sha256 = input.checksum_sha256.clone();
        let response_checksum_crc32 = input.checksum_crc32.clone();
        let response_checksum_crc32c = input.checksum_crc32c.clone();
        let response_checksum_crc64nvme = input.checksum_crc64nvme.clone();
        let response_checksum_sha1 = input.checksum_sha1.clone();
        // parent/filename 拆（WebDAV put_file 同式 ✗ init=父+名）
        // 条件写（`If-Match` / `If-None-Match` ✗ S3 现代并发控制）
        let cur = if self.has_current_delete_marker(&path).await? {
            None
        } else {
            self.etag_at(&path).await?
        };
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
        let (reader, md5_digest) = Md5Reader::new(stream_reader(blob));
        let result = if input.content_length.is_some() {
            self.upload
                .complete_upload_from_stream_with_md5(
                    &session.upload_id,
                    expected_sha256_hex.as_deref(),
                    expected_md5,
                    expected_crc32,
                    expected_crc32c,
                    expected_crc64nvme,
                    expected_sha1,
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
                    expected_sha1,
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
        let etag = finish_md5(&md5_digest);
        self.store_version_etag(&result.entry.id, &result.version.id, &etag)
            .await?;
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
            checksum_sha1: response_checksum_sha1,
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
        let (reader, md5_digest) = Md5Reader::new(content.reader);
        let result = self
            .upload
            .complete_upload_from_stream(
                &session.upload_id,
                None,
                Some("S3 COPY"),
                Box::new(reader),
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
        let etag = finish_md5(&md5_digest);
        self.store_version_etag(&result.entry.id, &result.version.id, &etag)
            .await?;
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

    /// 批量删除（逐键验证条件，再以一次事务批量创建删除标记）。
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
        let mut per_key_err: Vec<Option<String>> = vec![None; keys.len()];
        let mut valid_candidates = Vec::new();
        let mut etag_checks = Vec::new();
        let mut mtime_checks = Vec::new();
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
                            if (want_etag[i].is_some()
                                || want_mtime[i].is_some()
                                || want_size[i].is_some())
                                && self.has_current_delete_marker(&p).await?
                            {
                                per_key_err[i] = Some("PreconditionFailed".to_string());
                                continue;
                            }
                            if let Some(want) = &want_etag[i] {
                                let Some(version_id) = entry.current_version_id else {
                                    per_key_err[i] = Some("PreconditionFailed".to_string());
                                    continue;
                                };
                                etag_checks.push((
                                    i,
                                    entry.id,
                                    version_id.to_string().replace('-', ""),
                                    want.clone(),
                                ));
                            }
                            if let Some(want) = &want_mtime[i] {
                                mtime_checks.push((i, entry.id, want.clone()));
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
                            valid_candidates.push((i, p));
                        }
                        // 缺失 + 带条件 = 条件不可满足（幂等语义仅对**无条件**删适用）
                        Ok(None) => {
                            if want_etag[i].is_some()
                                || want_mtime[i].is_some()
                                || want_size[i].is_some()
                            {
                                per_key_err[i] = Some("PreconditionFailed".to_string());
                            } else {
                                valid_candidates.push((i, p));
                            }
                        }
                        Err(e) => per_key_err[i] = Some(e.to_string()),
                    }
                }
                Ok(_) => per_key_err[i] = Some("invalid key".to_string()),
                Err(e) => per_key_err[i] = Some(e.to_string()),
            }
        }

        if !mtime_checks.is_empty() {
            let entry_ids: Vec<_> = mtime_checks.iter().map(|(_, id, _)| *id).collect();
            let versions = self
                .entry_repo
                .find_current_versions_for_entries(&entry_ids)
                .await
                .map_err(dom_err)?;
            let current_mtimes: std::collections::HashMap<_, _> = versions
                .into_iter()
                .map(|version| (version.entry_id, Timestamp::from(version.created_at)))
                .collect();
            for (index, entry_id, expected) in mtime_checks {
                if !current_mtimes
                    .get(&entry_id)
                    .is_some_and(|actual| same_second(actual, &expected))
                {
                    per_key_err[index] = Some("PreconditionFailed".to_string());
                }
            }
        }

        // Put/Copy and multipart ETags are content-derived overrides, so compare them through one
        // batch property read instead of assuming the opaque version id is the object's ETag.
        if !etag_checks.is_empty() {
            let entry_ids: Vec<_> = etag_checks.iter().map(|(_, id, _, _)| *id).collect();
            let version_etags = self.load_version_etags(&entry_ids).await?;
            for (i, entry_id, version_id, expected) in etag_checks {
                let actual = version_etags
                    .get(&entry_id)
                    .and_then(|etags| etags.get(&version_id))
                    .map(String::as_str)
                    .unwrap_or(version_id.as_str());
                if actual != expected.as_str() {
                    per_key_err[i] = Some("PreconditionFailed".to_string());
                }
            }
        }
        let mut marker_ids_by_key = std::collections::HashMap::new();
        let mut marker_rows = Vec::new();
        let mut marker_indexes_by_key: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        let mut deleted_by_index = std::collections::HashMap::new();
        let mut seen_keys = std::collections::HashSet::new();
        for (i, path) in valid_candidates {
            if per_key_err[i].is_some() {
                continue;
            }
            let key = path.as_str().to_string();
            if let Some(version_id) = input.delete.objects[i].version_id.as_deref() {
                if self
                    .delete_markers
                    .delete(&self.namespace, &key, version_id)
                    .await
                    .map_err(dom_err)?
                {
                    deleted_by_index.insert(
                        i,
                        DeletedObject {
                            key: Some(keys[i].clone()),
                            version_id: Some(version_id.to_string()),
                            delete_marker: Some(true),
                            ..Default::default()
                        },
                    );
                    continue;
                }
                let version_id_parsed = match vfiles_domain::VersionId::from_string(version_id) {
                    Ok(id) => id,
                    Err(_) => {
                        per_key_err[i] = Some("NoSuchVersion".to_string());
                        continue;
                    }
                };
                let Some(entry) = self.entry_at(&path).await? else {
                    per_key_err[i] = Some("NoSuchVersion".to_string());
                    continue;
                };
                match self.entry_repo.find_version(&version_id_parsed).await {
                    Ok(version) if version.entry_id == entry.id => {}
                    _ => {
                        per_key_err[i] = Some("NoSuchVersion".to_string());
                        continue;
                    }
                }
                let versions = self
                    .entry_repo
                    .find_versions_for_entries(&[entry.id])
                    .await
                    .map_err(dom_err)?;
                if versions.len() == 1 {
                    if let Err(error) = self
                        .workspace
                        .delete_entries(
                            &self.namespace,
                            std::slice::from_ref(&path),
                            Some("S3 delete object version"),
                            &self.owner,
                        )
                        .await
                    {
                        per_key_err[i] = Some(format!("InternalError:{error}"));
                        continue;
                    }
                } else {
                    if !self
                        .entry_repo
                        .delete_version(&version_id_parsed)
                        .await
                        .map_err(dom_err)?
                    {
                        per_key_err[i] = Some("NoSuchVersion".to_string());
                        continue;
                    }
                    self.entry_repo
                        .remove_entry_property(
                            &entry.id,
                            &version_etag_property(&version_id_parsed),
                        )
                        .await
                        .map_err(dom_err)?;
                }
                deleted_by_index.insert(
                    i,
                    DeletedObject {
                        key: Some(keys[i].clone()),
                        version_id: Some(version_id.to_string()),
                        delete_marker: Some(false),
                        ..Default::default()
                    },
                );
            } else {
                marker_indexes_by_key
                    .entry(key.clone())
                    .or_default()
                    .push(i);
                if seen_keys.insert(key.clone()) {
                    let version_id = vfiles_domain::VersionId::new().to_string();
                    marker_ids_by_key.insert(key.clone(), version_id.clone());
                    marker_rows.push((key, version_id, time::OffsetDateTime::now_utc()));
                }
            }
        }
        if let Err(error) = self
            .delete_markers
            .create_many(&self.namespace, &self.owner, &marker_rows)
            .await
        {
            for indexes in marker_indexes_by_key.values() {
                for index in indexes {
                    per_key_err[*index] = Some(format!("InternalError:{error}"));
                }
            }
            marker_ids_by_key.clear();
        }

        for (i, k) in keys.iter().enumerate() {
            if let Some(msg) = &per_key_err[i] {
                let (code, text) = if msg == "PreconditionFailed" {
                    (
                        "PreconditionFailed".to_string(),
                        "a precondition on this object failed".to_string(),
                    )
                } else if msg == "NoSuchVersion" || msg == "InvalidRequest" {
                    (
                        msg.clone(),
                        "the requested object version cannot be deleted".to_string(),
                    )
                } else if let Some(text) = msg.strip_prefix("InternalError:") {
                    ("InternalError".to_string(), text.to_string())
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
            if !quiet {
                if let Some(item) = deleted_by_index.remove(&i) {
                    deleted.push(item);
                } else if let Some(version_id) = norm(k)
                    .ok()
                    .and_then(|path| marker_ids_by_key.get(path.as_str()).cloned())
                {
                    deleted.push(DeletedObject {
                        key: Some(k.clone()),
                        delete_marker: Some(true),
                        delete_marker_version_id: Some(version_id),
                        ..Default::default()
                    });
                }
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
        // Marker versions live separately from object versions and can be removed directly.
        if let Some(vid) = input.version_id.clone() {
            if self
                .delete_markers
                .delete(&self.namespace, path.as_str(), &vid)
                .await
                .map_err(dom_err)?
            {
                return ok(DeleteObjectOutput {
                    delete_marker: Some(true),
                    version_id: Some(vid),
                    ..Default::default()
                });
            }
            self.permanently_delete_version(&path, &vid).await?;
            return ok(DeleteObjectOutput {
                delete_marker: Some(false),
                version_id: Some(vid),
                ..Default::default()
            });
        }
        // Conditional delete applies to the current object; missing keys still create markers.
        let cur = if self.has_current_delete_marker(&path).await? {
            None
        } else {
            self.etag_at(&path).await?
        };
        check_dest_conditions(cur.as_deref(), input.if_match.as_ref(), None)?;
        let version_id = vfiles_domain::VersionId::new().to_string();
        let created_at = time::OffsetDateTime::now_utc();
        self.delete_markers
            .create(
                &self.namespace,
                path.as_str(),
                &self.owner,
                &version_id,
                created_at,
            )
            .await
            .map_err(dom_err)?;
        Ok(S3Response::new(DeleteObjectOutput {
            delete_marker: Some(true),
            version_id: Some(version_id),
            ..Default::default()
        }))
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

    /// 上传单个 part（partNumber 1..=10000 ✗ 内部索引 = partNumber-1）。
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
        let expected_sha1 = decode_checksum_sha1(input.checksum_sha1.as_deref())?;
        let response_checksum_sha256 = input.checksum_sha256.clone();
        let response_checksum_crc32 = input.checksum_crc32.clone();
        let response_checksum_crc32c = input.checksum_crc32c.clone();
        let response_checksum_crc64nvme = input.checksum_crc64nvme.clone();
        let response_checksum_sha1 = input.checksum_sha1.clone();
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
                expected_sha1,
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
            checksum_sha1: response_checksum_sha1,
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
            listed.push((
                number,
                etag.value().to_string(),
                decode_content_md5(part.checksum_md5.as_deref())?,
                decode_checksum_sha1(part.checksum_sha1.as_deref())?,
                decode_checksum_sha256(part.checksum_sha256.as_deref())?,
                decode_checksum_crc32(part.checksum_crc32.as_deref())?,
                decode_checksum_crc32c(part.checksum_crc32c.as_deref())?,
                decode_checksum_crc64nvme(part.checksum_crc64nvme.as_deref())?,
            ));
        }
        if listed.is_empty() {
            return Err(s3s::s3_error!(InvalidPart, "completed part list is empty"));
        }
        // Completion may select a subset of uploaded parts. S3 requires only an ordered list of
        // existing parts and a 5 MiB minimum for every selected part except the last.
        let requested_numbers: Vec<i32> = listed.iter().map(|(number, ..)| *number).collect();
        let stored_sizes: Vec<(i32, u64)> = stored
            .iter()
            .map(|part| ((part.part_index + 1) as i32, part.size_bytes.as_u64()))
            .collect();
        let selected_indices = resolve_completed_part_indices(&stored_sizes, &requested_numbers)?;
        // 客户端必须回显每个 UploadPart 返回的 ETag，且值需与已存分片内容一致。
        let mut part_md5s = Vec::with_capacity(listed.len());
        for (number, etag, md5, sha1, sha256, crc32, crc32c, crc64nvme) in listed {
            let part_index = (number - 1) as u32;
            let Some((reader, _size)) = self
                .upload
                .open_upload_part(&upload_id, part_index)
                .await
                .map_err(dom_err)?
            else {
                return Err(s3s::s3_error!(InvalidPart, "part {} is missing", number));
            };
            let actual = hash_upload_part(reader)
                .await
                .map_err(|e| s3s::s3_error!(InternalError, "read upload part: {}", e))?;
            if actual.md5_hex != etag {
                return Err(s3s::s3_error!(InvalidPart, "part {} etag mismatch", number));
            }
            if md5.is_some_and(|expected| expected != actual.md5)
                || sha1.is_some_and(|expected| expected != actual.sha1)
                || sha256.is_some_and(|expected| expected != actual.sha256)
                || crc32.is_some_and(|expected| expected != actual.crc32)
                || crc32c.is_some_and(|expected| expected != actual.crc32c)
                || crc64nvme.is_some_and(|expected| expected != actual.crc64nvme)
            {
                return Err(s3s::s3_error!(
                    BadDigest,
                    "part {} checksum mismatch",
                    number
                ));
            }
            part_md5s.push(actual.md5);
        }
        let etag = multipart_etag(&part_md5s);
        // 会话上存的 `x-amz-meta-*` 必须在完成**之前**读（完成会清掉会话目录）
        let custom = self
            .upload
            .get_upload_custom_metadata(&upload_id)
            .await
            .unwrap_or_default();
        let result = self
            .upload
            .complete_multipart_upload(&upload_id, &selected_indices, Some("S3 multipart"))
            .await
            .map_err(dom_err)?;
        self.store_version_etag(&result.entry.id, &result.version.id, &etag)
            .await?;
        // 会话上存的 `x-amz-meta-*` → 条目属性（完成即定稿 = 覆盖写语义）
        let map: s3s::dto::Metadata = custom.into_iter().collect();
        self.store_metadata(&result.entry.id, (!map.is_empty()).then_some(&map))
            .await?;
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

    #[test]
    fn listed_object_last_modified_uses_current_version_time() {
        let entry_created_at = time::OffsetDateTime::from_unix_timestamp(1_700_000_000)
            .expect("fixed timestamp is valid");
        let version_created_at = time::OffsetDateTime::from_unix_timestamp(1_710_000_000)
            .expect("fixed timestamp is valid");
        assert_eq!(
            object_last_modified(entry_created_at, Some(version_created_at)),
            version_created_at,
            "overwriting an object must advance its S3 LastModified timestamp"
        );
        assert_eq!(
            object_last_modified(entry_created_at, None),
            entry_created_at,
            "objects without version metadata use the entry creation time"
        );
    }

    #[tokio::test]
    async fn md5_reader_hashes_stream_without_changing_bytes() {
        let bytes = b"S3 content ETag";
        let (mut reader, digest) = Md5Reader::new(std::io::Cursor::new(bytes));
        let mut read_back = Vec::new();
        reader
            .read_to_end(&mut read_back)
            .await
            .expect("test operation should succeed");
        assert_eq!(read_back, bytes);
        assert_eq!(finish_md5(&digest), md5_hex(bytes));
    }

    #[test]
    fn reading_a_delete_marker_returns_method_not_allowed_header() {
        let error = delete_marker_read_error();
        assert_eq!(*error.code(), s3s::S3ErrorCode::MethodNotAllowed);
        assert_eq!(
            error
                .headers()
                .and_then(|headers| headers.get("x-amz-delete-marker"))
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
    }

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
        let len = 10;
        assert_eq!(
            copy_range_bounds("bytes=0-3", len).expect("test operation should succeed"),
            0..4
        );
        assert_eq!(
            copy_range_bounds("bytes=5-", len).expect("test operation should succeed"),
            5..10
        );
        assert_eq!(
            copy_range_bounds("bytes=-3", len).expect("test operation should succeed"),
            7..10
        );
        assert_eq!(
            copy_range_bounds("bytes=8-99", len).expect("test operation should succeed"),
            8..10,
            "尾越界夹取"
        );
        assert!(copy_range_bounds("bytes=10-12", len).is_err(), "起点越界");
        assert!(copy_range_bounds("bytes=5-2", len).is_err(), "逆序");
        assert!(copy_range_bounds("0-3", len).is_err(), "缺 bytes= 前缀");
        assert!(copy_range_bounds("bytes=0-0", 0).is_err(), "空源");
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
                .map(|o| o.key.clone().expect("test operation should succeed"))
                .collect::<Vec<_>>(),
            vec!["a.txt", "b.txt"]
        );
        assert!(p1.truncated);
        assert_eq!(p1.next.as_deref(), Some("b.txt"));
        let p2 = paginate(entries, p1.next.as_deref(), 2);
        assert_eq!(
            p2.contents
                .iter()
                .map(|o| o.key.clone().expect("test operation should succeed"))
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
        assert_eq!(
            resolve_max_keys(None).expect("test operation should succeed"),
            1000
        );
        assert_eq!(
            resolve_max_keys(Some(0)).expect("test operation should succeed"),
            0
        );
        assert_eq!(
            resolve_max_keys(Some(5000)).expect("test operation should succeed"),
            1000
        );
        assert!(
            resolve_max_keys(Some(-1)).is_err(),
            "负值 = InvalidArgument"
        );
    }

    #[test]
    fn range_resolution_matches_http_semantics() {
        let size = 100u64;
        let int = s3s::dto::Range::parse("bytes=0-9").expect("test operation should succeed");
        assert_eq!(
            resolve_range(Some(int), size).expect("test operation should succeed"),
            Some(0..10)
        );
        let open = s3s::dto::Range::parse("bytes=90-").expect("test operation should succeed");
        assert_eq!(
            resolve_range(Some(open), size).expect("test operation should succeed"),
            Some(90..100)
        );
        let suffix = s3s::dto::Range::parse("bytes=-10").expect("test operation should succeed");
        assert_eq!(
            resolve_range(Some(suffix), size).expect("test operation should succeed"),
            Some(90..100)
        );
        let past = s3s::dto::Range::parse("bytes=200-").expect("test operation should succeed");
        assert!(resolve_range(Some(past), size).is_err(), "越界 = 416");
        assert_eq!(
            resolve_range(None, size).expect("test operation should succeed"),
            None
        );
    }

    #[test]
    fn multipart_etag_uses_part_md5_digest_and_part_count() {
        let part_md5s = [
            hex::decode("5d41402abc4b2a76b9719d911017c592")
                .expect("test operation should succeed")
                .try_into()
                .expect("test operation should succeed"),
            hex::decode("7d793037a0760186574b0282f2f435e7")
                .expect("test operation should succeed")
                .try_into()
                .expect("test operation should succeed"),
        ];
        assert_eq!(
            multipart_etag(&part_md5s),
            "065947336a2f2a95ba8899f3675c3be6-2"
        );
    }

    #[test]
    fn multipart_completion_selects_ordered_existing_parts() {
        let stored = [(1, 5 * 1024 * 1024), (2, 9), (3, 4 * 1024 * 1024)];
        assert_eq!(
            resolve_completed_part_indices(&stored, &[1, 3])
                .expect("a subset of uploaded parts can be completed"),
            [0, 2]
        );
        assert_eq!(
            resolve_completed_part_indices(&stored, &[3]).expect("a small final part is allowed"),
            [2]
        );
        assert!(resolve_completed_part_indices(&stored, &[1, 4]).is_err());
        assert!(resolve_completed_part_indices(&stored, &[3, 1]).is_err());
        assert!(resolve_completed_part_indices(&stored, &[2, 3]).is_err());
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
