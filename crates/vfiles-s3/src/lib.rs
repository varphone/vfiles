//! S3 兼容 API（协议族并列 crate ✗ round r1 目标首件 · 近期路线第 1 件）。
//!
//! 形态：独立端口（S3 path-style 寻址与前端根语义冲突 ✗ MinIO 同款 9000 独立端口直觉 ✓）+
//! 单虚拟桶 `default`（严格语义：非 default 桶 → NoSuchBucket ✓ 客户端列桶自配对）+
//! 认证 = SigV4（s3s 内建 ✗ env 静态单对 `VFILES_S3_ACCESS_KEY/SECRET_KEY`，未设随机
//! 生成 + warn 打印 = 零配置试用 ✓ per-user 凭证 = 扩展债记档）。
//!
//! 映射：key = 默认 ns 根下相对路径（tree 展开为 flat keys ✗ 版本链天然 = ETag =
//! current_version_id hex 引号（与 WebDAV r14 同式 ✓ 跨协议一致））。r1 记档债：
//! put 聚合（流式直连 = P2 ✗ 与 WebDAV GET 流式同级优化）、ListObjects 续页/delimiter
//! 组前缀（r1 全量截 1000）、last_modified 未接（P2 接版本时间）。

use async_trait::async_trait;
use tokio::io::AsyncReadExt;

use s3s::dto::{
    DeleteObjectInput, DeleteObjectOutput, GetObjectInput, GetObjectOutput, HeadObjectOutput,
    HeadObjectInput, ListBucketsOutput, ListObjectsV2Input, ListObjectsV2Output,
    PutObjectInput, PutObjectOutput,
    Bucket, Object, StreamingBlob,
};
use s3s::{S3, S3Request, S3Response, S3Result};

/// 默认（唯一）虚拟桶名。
pub const DEFAULT_BUCKET: &str = "default";

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
    // protocol.rs:179 官方构造（手填 status/headers 缺 extensions ✗ 编译错即证）
    Ok(S3Response::new(output))
}

/// DomainErr → S3 错（NotFound/NoSuchKey / 其余 InternalError ✗ 消息带因）。
fn dom_err(e: vfiles_domain::DomainError) -> s3s::S3Error {
    match e {
        vfiles_domain::DomainError::NotFound { .. } => {
            s3s::s3_error!(NoSuchKey, "No such key")
        }
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

/// 递归展平默认 ns 全部文件 key（= path_norm 相对形 ✓ 目录不产出对象 ✗ 树展开 =
/// r1 无续页全量（截 1000 由调用方 ✓ 记档分页/delimiter = P2 债）。
async fn collect_keys(
    repo: &std::sync::Arc<dyn vfiles_domain::EntryRepo + Send + Sync>,
    ns: &vfiles_domain::NamespaceId,
    prefix_path: &vfiles_domain::NormalizedPath,
    out: &mut Vec<(String, Option<u64>)>,
) -> vfiles_domain::DomainResult<()> {
    let children = repo.find_children(ns, prefix_path).await?;
    for child in children {
        if matches!(child.entry_type, vfiles_domain::EntryKind::Directory) {
            Box::pin(collect_keys(repo, ns, &child.path_norm, out)).await?;
        } else {
            out.push((child.path_norm.as_str().to_string(), None));
        }
    }
    Ok(())
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
        let mut out = ListBucketsOutput::default();
        out.buckets = Some(vec![Bucket {
            name: Some(DEFAULT_BUCKET.to_string()),
            ..Default::default()
        }]);
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
        let root = vfiles_domain::NormalizedPath::new("")
            .map_err(|e| s3s::s3_error!(InvalidArgument, "{}", e))?;
        let mut raw = Vec::new();
        collect_keys(&self.entry_repo, &self.namespace, &root, &mut raw)
            .await
            .map_err(dom_err)?;
        let prefix = input.prefix.unwrap_or_default();
        let mut keys: Vec<String> = raw
            .into_iter()
            .map(|(k, _)| k)
            .filter(|k| k.starts_with(&prefix))
            .collect();
        keys.sort();
        let max = input.max_keys.unwrap_or(1000).min(1000) as usize;
        let truncated = keys.len() > max;
        keys.truncate(max);
        let mut out = ListObjectsV2Output::default();
        out.name = Some(input.bucket);
        out.prefix = if prefix.is_empty() { None } else { Some(prefix) };
        out.key_count = Some(keys.len() as i32);
        out.is_truncated = Some(truncated);
        out.contents = Some(
            keys.into_iter()
                .map(|key| Object {
                    key: Some(key),
                    ..Default::default()
                })
                .collect(),
        );
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
        // ETag = current_version_id hex 引号（r1 设计、r2 补赋值 ✗ 与 WebDAV derive_etag
        // 跨协议同式 ✗ find + open 双查 = r1 注记的已知容忍）
        let entry = self
            .entry_repo
            .find_by_path(&self.namespace, &path)
            .await
            .map_err(dom_err)?
            .ok_or_else(|| s3s::s3_error!(NoSuchKey, "No such key"))?;
        // None vid = 首版本未定形?Entries 恒有 vid（versions 链必建）✗ None → 空串防呆
        // Strong 变体序列化自附引号 ✗ 存裸 hex 防双引（etag.rs:19-24 + 编译器式）
        let etag = entry
            .current_version_id
            .map(|v| v.to_string().replace('-', ""))
            .unwrap_or_default();
        let file = self
            .workspace
            .open_file(&self.namespace, &path, None)
            .await
            .map_err(dom_err)?;
        let mut data = Vec::with_capacity(file.size_bytes as usize);
        let mut reader = file.reader;
        reader.read_to_end(&mut data).await.map_err(|e| {
            s3s::s3_error!(InternalError, "read failed: {}", e)
        })?;
        let mut out = GetObjectOutput::default();
        out.body = Some(StreamingBlob::from_bytes(data.into()));
        out.content_length = Some(file.size_bytes as i64);
        out.content_type = file.mime_type;
        out.accept_ranges = Some("bytes".to_string());
        out.e_tag = Some(s3s::dto::ETag::Strong(etag));
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
        // ETag 同 get（HEAD 头客户端同需 ✗ r2 补）
        let entry = self
            .entry_repo
            .find_by_path(&self.namespace, &path)
            .await
            .map_err(dom_err)?
            .ok_or_else(|| s3s::s3_error!(NoSuchKey, "No such key"))?;
        // Strong 变体序列化自附引号 ✗ 存裸 hex 防双引（etag.rs:19-24 + 编译器式）
        let etag = entry
            .current_version_id
            .map(|v| v.to_string().replace('-', ""))
            .unwrap_or_default();
        let file = self
            .workspace
            .open_file(&self.namespace, &path, None)
            .await
            .map_err(dom_err)?;
        let mut out = HeadObjectOutput::default();
        out.content_length = Some(file.size_bytes as i64);
        out.content_type = file.mime_type;
        out.accept_ranges = Some("bytes".to_string());
        out.e_tag = Some(s3s::dto::ETag::Strong(etag));
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
        // r1 记档债：body 聚合（流式直连 = P2 ✗ 与 WebDAV 流式同级优化）
        let blob = input.body.unwrap_or_else(|| StreamingBlob::from_bytes(Default::default()));
        let mut data: Vec<u8> = Vec::new();
        let mut stream = std::pin::pin!(blob);
        while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
            let chunk = chunk.map_err(|e| s3s::s3_error!(InternalError, "body: {}", e))?;
            data.extend_from_slice(&chunk);
        }
        let path = norm(&input.key).map_err(dom_err)?;
        // parent/filename 拆（WebDAV put_file 同式 ✗ r110'c 语义：init=父+名）
        let full = path.as_str();
        let (parent_str, filename) = match full.rsplit_once('/') {
            Some((dir, name)) => (dir.to_string(), name.to_string()),
            None => (String::new(), full.to_string()),
        };
        let filename = if filename.is_empty() { "upload".to_string() } else { filename };
        let parent = vfiles_domain::NormalizedPath::new(&parent_str)
            .map_err(|e| s3s::s3_error!(InvalidArgument, "{}", e))?;
        let session = self
            .upload
            .init_upload(
                &self.namespace,
                &parent,
                &filename,
                data.len() as u64,
                input.content_type.as_deref(), // ContentType=String（content_type.rs:4）→ init 要 &str（as_deref 编译器式 ✓ 透传保留 = P2 债免记）
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
            .map_err(dom_err)?;
        let out = PutObjectOutput::default();
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
            Ok(_) | Err(vfiles_domain::DomainError::NotFound { .. }) => ok(DeleteObjectOutput::default()),
            Err(e) => Err(dom_err(e)),
        }
    }
}
