//! S3 兼容 API（协议族并列 crate ✗ round r1 目标首件 · 近期路线第 1 件）。
//!
//! 形态：独立端口（S3 path-style 寻址与前端根语义冲突 ✗ MinIO 同款 9000 独立端口直觉 ✓）+
//! 单虚拟桶 `default`（严格语义：非 default 桶 → NoSuchBucket ✓ 客户端列桶自配对）+
//! 认证 = SigV4（s3s 内建 ✗ env 静态单对 `VFILES_S3_ACCESS_KEY/SECRET_KEY`，未设随机
//! 生成 + warn 打印 = 零配置试用 ✓ per-user 凭证 = 扩展债记档）。
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
//! 扩展债（记档待排期）：put 流式直连（r1 聚合 Vec）· multipart upload · per-user 凭证 ·
//! region 校验 · 大桶 SQL 分页（现全量枚举后内存分页）。

use async_trait::async_trait;
use s3s::dto::{
    AbortMultipartUploadInput, AbortMultipartUploadOutput, Bucket, CommonPrefix,
    CompleteMultipartUploadInput, CompleteMultipartUploadOutput, CopyObjectInput, CopyObjectOutput,
    CopyObjectResult, CreateMultipartUploadInput, CreateMultipartUploadOutput, DeleteObjectInput,
    DeleteObjectOutput, DeleteObjectsInput, DeleteObjectsOutput, DeletedObject,
    Error as S3DeleteError, GetObjectInput, GetObjectOutput, HeadObjectInput, HeadObjectOutput,
    ListBucketsOutput, ListMultipartUploadsInput, ListMultipartUploadsOutput, ListObjectsInput,
    ListObjectsOutput, ListObjectsV2Input, ListObjectsV2Output, ListPartsInput, ListPartsOutput,
    MultipartUpload, Object, Part, PutObjectInput, PutObjectOutput, StreamingBlob, Timestamp,
    UploadPartInput, UploadPartOutput,
};
use s3s::{S3, S3Request, S3Response, S3Result};
use tokio::io::AsyncReadExt;

/// 默认（唯一）虚拟桶名。
pub const DEFAULT_BUCKET: &str = "default";
/// 单页上限（S3 硬上限）。
const MAX_KEYS_LIMIT: usize = 1000;

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

/// 默认 ns 全部**文件** key（目录不产出对象 ✓ = S3 语义），带 size/mtime/ETag。
///
/// r13：`files_with_meta` **一条 SQL** 取全（此前逐目录递归 = 目录数条查询 ✗ 大桶 N+1）。
async fn collect_objects(
    repo: &std::sync::Arc<dyn vfiles_domain::EntryRepo + Send + Sync>,
    ns: &vfiles_domain::NamespaceId,
) -> vfiles_domain::DomainResult<Vec<ObjMeta>> {
    let mut out: Vec<ObjMeta> = repo
        .files_with_meta(ns)
        .await?
        .into_iter()
        .map(|m| ObjMeta {
            key: m.entry.path_norm.as_str().to_string(),
            size: m.size_bytes.unwrap_or(0),
            last_modified: Timestamp::from(m.entry.created_at),
            etag: m
                .entry
                .current_version_id
                .map(|v| v.to_string().replace('-', ""))
                .unwrap_or_default(),
        })
        .collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
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

/// 聚合请求体（未知长度 PUT / UploadPart 回退路径 ✗ 流式直连为默认）。
async fn read_body(blob: Option<StreamingBlob>) -> S3Result<Vec<u8>> {
    let blob = blob.unwrap_or_else(|| StreamingBlob::from_bytes(Default::default()));
    let mut data: Vec<u8> = Vec::new();
    let mut stream = std::pin::pin!(blob);
    while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
        let chunk = chunk.map_err(|e| s3s::s3_error!(InternalError, "body: {}", e))?;
        data.extend_from_slice(&chunk);
    }
    Ok(data)
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

/// part 数据 MD5 十六进制（S3 `UploadPart` 返回的 ETag 形）。
fn md5_hex(data: &[u8]) -> String {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(data);
    hex::encode(h.finalize())
}

impl std::fmt::Debug for VfilesS3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // workspace/upload 等非 Debug ✗ 标准省内容式（-W missing-debug-implementations 清零）
        f.debug_struct("VfilesS3").finish_non_exhaustive()
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

        let all = collect_objects(&self.entry_repo, &self.namespace)
            .await
            .map_err(dom_err)?;
        let entries = build_entries(&all, &prefix, delimiter.as_deref());
        let page = paginate(entries, after.as_deref(), max);

        let out = ListObjectsV2Output {
            name: Some(input.bucket),
            prefix: non_empty(Some(prefix)),
            delimiter,
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

        let all = collect_objects(&self.entry_repo, &self.namespace)
            .await
            .map_err(dom_err)?;
        let entries = build_entries(&all, &prefix, delimiter.as_deref());
        let page = paginate(entries, marker.as_deref(), max);

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
        let etag = entry
            .current_version_id
            .map(|v| v.to_string().replace('-', ""))
            .unwrap_or_default();
        let last_modified = Timestamp::from(entry.created_at);
        let file = self
            .workspace
            .open_file(&self.namespace, &path, None)
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
        let out = GetObjectOutput {
            body: Some(StreamingBlob::from_bytes(body.into())),
            content_length: Some(content_length),
            content_type: file.mime_type,
            accept_ranges: Some("bytes".to_string()),
            content_range,
            e_tag: Some(s3s::dto::ETag::Strong(etag)),
            last_modified: Some(last_modified),
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
        let etag = entry
            .current_version_id
            .map(|v| v.to_string().replace('-', ""))
            .unwrap_or_default();
        let last_modified = Timestamp::from(entry.created_at);
        let file = self
            .workspace
            .open_file(&self.namespace, &path, None)
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
        let out = HeadObjectOutput {
            content_length: Some(content_length),
            content_type: file.mime_type,
            accept_ranges: Some("bytes".to_string()),
            content_range,
            e_tag: Some(s3s::dto::ETag::Strong(etag)),
            last_modified: Some(last_modified),
            ..Default::default()
        };
        ok(out)
    }

    async fn put_object(
        &self,
        req: S3Request<PutObjectInput>,
    ) -> S3Result<S3Response<PutObjectOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let path = norm(&input.key).map_err(dom_err)?;
        // parent/filename 拆（WebDAV put_file 同式 ✗ init=父+名）
        let (parent_str, filename) = split_key(path.as_str());
        let parent = vfiles_domain::NormalizedPath::new(&parent_str)
            .map_err(|e| s3s::s3_error!(InvalidArgument, "{}", e))?;
        // r6：Content-Length 已知 → **流式直连**（不再全量入内存）；未知 → 回退聚合
        let declared = input.content_length.unwrap_or(0);
        let result = if declared > 0 {
            let session = self
                .upload
                .init_upload(
                    &self.namespace,
                    &parent,
                    &filename,
                    declared as u64,
                    input.content_type.as_deref(),
                    None,
                    &self.owner,
                )
                .await
                .map_err(dom_err)?;
            let blob = input
                .body
                .unwrap_or_else(|| StreamingBlob::from_bytes(Default::default()));
            let reader = stream_reader(blob);
            self.upload
                .complete_upload_from_stream(
                    &session.upload_id,
                    None,
                    Some("S3 PUT"),
                    Box::new(reader),
                )
                .await
                .map_err(dom_err)?
        } else {
            let data = read_body(input.body).await?;
            let session = self
                .upload
                .init_upload(
                    &self.namespace,
                    &parent,
                    &filename,
                    data.len() as u64,
                    input.content_type.as_deref(),
                    None,
                    &self.owner,
                )
                .await
                .map_err(dom_err)?;
            self.upload
                .complete_upload_from_stream(
                    &session.upload_id,
                    None,
                    Some("S3 PUT"),
                    Box::new(std::io::Cursor::new(data)),
                )
                .await
                .map_err(dom_err)?
        };
        let etag = result.version.id.to_string().replace('-', "");
        let out = PutObjectOutput {
            e_tag: Some(s3s::dto::ETag::Strong(etag)),
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
        let content = self
            .workspace
            .read_file_bytes(&self.namespace, &src_path, None)
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
        let (parent_str, filename) = split_key(dst_path.as_str());
        let parent = vfiles_domain::NormalizedPath::new(&parent_str)
            .map_err(|e| s3s::s3_error!(InvalidArgument, "{}", e))?;
        let session = self
            .upload
            .init_upload(
                &self.namespace,
                &parent,
                &filename,
                content.bytes.len() as u64,
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
                Box::new(std::io::Cursor::new(content.bytes)),
            )
            .await
            .map_err(dom_err)?;
        let etag = result.version.id.to_string().replace('-', "");
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
        for (i, k) in keys.iter().enumerate() {
            match norm(k) {
                Ok(p) if !p.as_str().is_empty() => {
                    match self.entry_repo.find_by_path(&self.namespace, &p).await {
                        Ok(Some(_)) => {
                            if seen.insert(p.as_str().to_string()) {
                                existing.push(p);
                            }
                        }
                        Ok(None) => {}
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
                errors.push(S3DeleteError {
                    key: Some(k.clone()),
                    code: Some("InvalidArgument".to_string()),
                    message: Some(msg.clone()),
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
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let path = norm(&input.key).map_err(dom_err)?;
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
        let data = read_body(input.body).await?;
        let etag = md5_hex(&data);
        self.upload
            .upload_part(&upload_id, (input.part_number - 1) as u32, &data)
            .await
            .map_err(dom_err)?;
        let out = UploadPartOutput {
            e_tag: Some(s3s::dto::ETag::Strong(etag)),
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
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let upload_id = parse_upload_id(&input.upload_id)?;
        let stored = self
            .upload
            .list_upload_parts(&upload_id)
            .await
            .map_err(dom_err)?;
        let have: std::collections::BTreeSet<i32> =
            stored.iter().map(|p| p.part_index as i32 + 1).collect();
        if have.is_empty() {
            return Err(s3s::s3_error!(InvalidPart, "no parts uploaded"));
        }
        if let Some(mpu) = &input.multipart_upload {
            let listed: std::collections::BTreeSet<i32> = mpu
                .parts
                .iter()
                .flatten()
                .filter_map(|p| p.part_number)
                .collect();
            // 拼接按全部已存分片进行 ✗ 列出集合必须与已存集合一致（否则内容会静默错位）
            if listed != have {
                return Err(s3s::s3_error!(
                    InvalidPart,
                    "the listed parts do not match the uploaded parts"
                ));
            }
            // ETag 校验（客户端回显的 part ETag = MD5(分片) ✗ 不符即拒）
            for p in mpu.parts.iter().flatten() {
                let Some(n) = p.part_number else { continue };
                let Some(etag) = &p.e_tag else { continue };
                match self
                    .upload
                    .read_upload_part(&upload_id, (n - 1) as u32)
                    .await
                    .map_err(dom_err)?
                {
                    Some(bytes) if md5_hex(&bytes) == etag.value() => {}
                    _ => {
                        return Err(s3s::s3_error!(InvalidPart, "part {} etag mismatch", n));
                    }
                }
            }
        }
        let result = self
            .upload
            .complete_multipart_upload(&upload_id, Some("S3 multipart"))
            .await
            .map_err(dom_err)?;
        let etag = result.version.id.to_string().replace('-', "");
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
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let upload_id = parse_upload_id(&input.upload_id)?;
        match self.upload.cancel_upload(&upload_id).await {
            Ok(()) | Err(vfiles_domain::DomainError::NotFound { .. }) => {
                ok(AbortMultipartUploadOutput::default())
            }
            Err(e) => Err(dom_err(e)),
        }
    }

    /// 列出已上传 part（partNumber = 内部索引+1 ✗ size 可读；part ETag 未持久化 = 记档债）。
    async fn list_parts(
        &self,
        req: S3Request<ListPartsInput>,
    ) -> S3Result<S3Response<ListPartsOutput>> {
        let input = req.input;
        if input.bucket != DEFAULT_BUCKET {
            return Err(s3s::s3_error!(NoSuchBucket, "bucket not found"));
        }
        let upload_id = parse_upload_id(&input.upload_id)?;
        let stored = self
            .upload
            .list_upload_parts(&upload_id)
            .await
            .map_err(dom_err)?;
        let mut parts: Vec<Part> = Vec::with_capacity(stored.len());
        for p in &stored {
            let e_tag = match self.upload.read_upload_part(&upload_id, p.part_index).await {
                Ok(Some(bytes)) => Some(s3s::dto::ETag::Strong(md5_hex(&bytes))),
                _ => None,
            };
            parts.push(Part {
                part_number: Some(p.part_index as i32 + 1),
                size: Some(p.size_bytes.as_u64() as i64),
                last_modified: Some(Timestamp::from(p.received_at)),
                e_tag,
                ..Default::default()
            });
        }
        let out = ListPartsOutput {
            bucket: Some(input.bucket),
            key: Some(input.key),
            upload_id: Some(input.upload_id),
            parts: Some(parts),
            is_truncated: Some(false),
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
        let sessions = self
            .upload
            .list_upload_sessions(&self.namespace)
            .await
            .map_err(dom_err)?;
        let mut items: Vec<(String, String, Timestamp)> = sessions
            .into_iter()
            .filter(|s| s.state == vfiles_domain::UploadState::Receiving)
            .filter_map(|s| {
                let key = if s.target_path_norm.as_str().is_empty() {
                    s.filename.clone()
                } else {
                    format!("{}/{}", s.target_path_norm.as_str(), s.filename)
                };
                key.starts_with(&prefix)
                    .then(|| (key, s.id.to_string(), Timestamp::from(s.created_at)))
            })
            .collect();
        // 续页游标（key-marker + upload-id-marker ✗ 同 key 多会话时用 id 定序）
        let after_key = input.key_marker.clone();
        let after_uid = input.upload_id_marker.clone();
        items.retain(|(k, id, _)| match (&after_key, &after_uid) {
            (Some(km), Some(um)) => {
                k.as_str() > km.as_str() || (k == km && id.as_str() > um.as_str())
            }
            (Some(km), None) => k.as_str() > km.as_str(),
            _ => true,
        });
        items.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let delim = input.delimiter.clone();
        let mut combined: Vec<(String, Option<MultipartUpload>)> = Vec::new();
        for (key, id, created) in items {
            if let Some(d) = &delim {
                let rest = &key[prefix.len()..];
                if let Some(pos) = rest.find(d.as_str()) {
                    let cp = format!("{}{}{}", prefix, &rest[..pos], d);
                    if !combined.iter().any(|(c, _)| c == &cp) {
                        combined.push((cp, None));
                    }
                    continue;
                }
            }
            combined.push((
                key.clone(),
                Some(MultipartUpload {
                    key: Some(key),
                    upload_id: Some(id),
                    initiated: Some(created),
                    ..Default::default()
                }),
            ));
        }
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
