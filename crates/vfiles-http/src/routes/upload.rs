//! File upload routes.

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    routing::{post, put},
};
use axum_extra::extract::{Multipart, cookie::CookieJar};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use crate::{
    AppState,
    dto::UploadRequest,
    error::{ApiError, ApiJson, ApiResult},
    routes::protected_request_context,
};
use vfiles_domain::{DomainError, NewAuditLog, NormalizedPath, UploadId};

#[derive(Debug, Deserialize)]
struct CompleteUploadRequest {
    message: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/upload", post(upload_file).put(put_upload))
        // curl -T 会把本地文件名拼到 URL 后面：PUT /api/files/upload/ci/app.tar.gz
        .route("/upload/{*path}", put(put_upload_path))
        .route("/upload/init", post(create_upload))
        .route(
            "/upload/chunks/{upload_id}/{chunk_index}",
            put(upload_chunk),
        )
        .route("/upload/complete/{upload_id}", post(complete_upload))
        .layer(DefaultBodyLimit::disable())
}

/// 上传体积上限：取「单文件配置上限」与「上传硬上限」中较小者。
///
/// `VFILES_MAX_FILE_SIZE_MB`（`features.max_file_size_bytes`）是界面上展示给用户的
/// 单文件上限，之前只在客户端预检、服务端并未强制；这里让所有上传路径（multipart /
/// 分片 / 原始 body）都按它校验。
fn max_upload_size_bytes(state: &AppState) -> u64 {
    state
        .config
        .limits
        .max_upload_size_bytes
        .min(state.config.features.max_file_size_bytes)
}

async fn create_upload(
    jar: CookieJar,
    axum::extract::State(state): axum::extract::State<AppState>,
    ApiJson(req): ApiJson<UploadRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let ctx = protected_request_context(&state, &jar).await?;
    tracing::info!(
        "Creating upload session for file: {} (size: {} bytes)",
        req.filename,
        req.size_bytes
    );

    // Validate filename
    if req.filename.is_empty()
        || req.filename.contains('/')
        || req.filename.contains('\\')
        || req.filename == "."
        || req.filename == ".."
    {
        return Err(ApiError::Domain(DomainError::Validation {
            message: "Invalid filename".to_string(),
        }));
    }

    // Validate file size
    let max_upload_size = max_upload_size_bytes(&state);
    if req.size_bytes > max_upload_size {
        return Err(ApiError::Domain(DomainError::Validation {
            message: format!("File too large. Maximum size is {} bytes", max_upload_size),
        }));
    }

    let target_path = NormalizedPath::new(&req.path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    let chunk_size = req
        .chunk_size
        .unwrap_or(state.config.limits.upload_chunk_size_bytes);

    let upload = state
        .upload_service
        .init_upload(
            &ctx.namespace_id,
            &target_path,
            &req.filename,
            req.size_bytes,
            None,
            Some(chunk_size),
            &ctx.actor_user_id,
        )
        .await?;

    tracing::info!("Upload session created with ID: {}", upload.upload_id);

    Ok(Json(serde_json::json!({
        "upload_id": upload.upload_id.to_string(),
        "chunk_size": upload.chunk_size,
        "total_chunks": upload.total_parts,
    })))
}

async fn ensure_upload_owner(
    state: &AppState,
    jar: &CookieJar,
    upload_id: &UploadId,
) -> ApiResult<()> {
    let ctx = protected_request_context(state, jar).await?;
    let session = state.upload_store.get_upload_session(upload_id).await?;

    if session.owner_user_id != ctx.actor_user_id {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }

    Ok(())
}

async fn upload_chunk(
    jar: CookieJar,
    axum::extract::Path((upload_id_str, chunk_index_str)): axum::extract::Path<(String, String)>,
    axum::extract::State(state): axum::extract::State<AppState>,
    body: axum::body::Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let upload_id = upload_id_str
        .parse::<uuid::Uuid>()
        .map(vfiles_domain::UploadId::from_uuid)
        .map_err(|_| {
            ApiError::Domain(DomainError::Validation {
                message: "Invalid upload ID format".to_string(),
            })
        })?;

    let chunk_index: u32 = chunk_index_str.parse().map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid chunk index".to_string(),
        })
    })?;

    ensure_upload_owner(&state, &jar, &upload_id).await?;

    tracing::debug!(
        "Uploading chunk {} for upload {} (size: {} bytes)",
        chunk_index,
        upload_id,
        body.len()
    );

    state
        .upload_service
        .upload_part(&upload_id, chunk_index, &body)
        .await?;

    tracing::debug!("Chunk {} uploaded successfully", chunk_index);

    Ok(Json(serde_json::json!({
        "chunk_index": chunk_index,
        "received": true,
    })))
}

async fn complete_upload(
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    axum::extract::Path(upload_id_str): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<AppState>,
    req: Option<Json<CompleteUploadRequest>>,
) -> ApiResult<Json<serde_json::Value>> {
    let upload_id = upload_id_str
        .parse::<uuid::Uuid>()
        .map(vfiles_domain::UploadId::from_uuid)
        .map_err(|_| {
            ApiError::Domain(DomainError::Validation {
                message: "Invalid upload ID format".to_string(),
            })
        })?;

    ensure_upload_owner(&state, &jar, &upload_id).await?;
    // 取上下文用于审计（带用户名快照）
    let ctx = crate::routes::protected_request_context(&state, &jar).await?;

    tracing::info!("Completing upload session: {}", upload_id);
    let message = req.and_then(|Json(body)| body.message);
    let completed = state
        .upload_service
        .complete_upload(&upload_id, None, message.as_deref())
        .await?;
    tracing::info!("Upload session {} completed successfully", upload_id);

    crate::audit::record_for(
        &state,
        &headers,
        &ctx,
        NewAuditLog::success(crate::audit::action::FILE_UPLOAD)
            .target(completed.entry.path_norm.as_str())
            .detail(format!(
                "上传文件（{} 字节）",
                completed.version.size_bytes.as_u64()
            )),
    )
    .await;

    Ok(Json(serde_json::json!({
        "upload_id": upload_id.to_string(),
        "completed": true,
        "path": completed.entry.path_norm.as_str(),
        "version_id": completed.version.id.to_string(),
    })))
}

/// `PUT /api/files/upload` 的查询参数。
#[derive(Debug, Deserialize)]
struct PutUploadQuery {
    /// 目标目录；若省略 `filename`，则把 `path` 的最后一段当作文件名。
    path: Option<String>,
    /// 文件名（与 `path` 一起构成目标位置）。
    filename: Option<String>,
    /// 版本说明。
    message: Option<String>,
}

/// 解析目标：`filename` 优先；否则用 `path` 的最后一段当文件名。
///
/// `?path=ci/` 这种显式目录（末尾带斜杠）需要配合 `filename` 使用。
fn split_target(path: &str, filename: Option<&str>) -> (String, String) {
    match filename.map(str::trim).filter(|value| !value.is_empty()) {
        Some(filename) => (path.trim_end_matches('/').to_string(), filename.to_string()),
        None => {
            let trimmed = path.trim_end_matches('/');
            match trimmed.rsplit_once('/') {
                Some((parent, name)) => (parent.to_string(), name.to_string()),
                None if !trimmed.is_empty() => (String::new(), trimmed.to_string()),
                None => (String::new(), String::new()),
            }
        }
    }
}

/// `PUT /api/files/upload/<命名空间内路径>`：把 URL 路径整段当作目标位置。
async fn put_upload_path(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    axum::extract::Path(path): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<PutUploadQuery>,
    request: axum::extract::Request,
) -> ApiResult<Json<serde_json::Value>> {
    let (directory, filename) = split_target(&path, query.filename.as_deref());
    put_upload_inner(
        state,
        jar,
        headers,
        directory,
        filename,
        query.message,
        request,
    )
    .await
}

/// 单请求上传（原始 body）：`curl -T 文件 -H "Authorization: Bearer …" "…/upload?path=ci"`。
///
/// 与 multipart 版本一样流式落盘、边写边校验大小上限，内部仍复用
/// `init_upload` + `complete_upload_from_stream`，因此版本历史、快照与审计行为一致。
async fn put_upload(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    axum::extract::Query(query): axum::extract::Query<PutUploadQuery>,
    request: axum::extract::Request,
) -> ApiResult<Json<serde_json::Value>> {
    let (directory, filename) = split_target(
        &query.path.clone().unwrap_or_default(),
        query.filename.as_deref(),
    );
    put_upload_inner(
        state,
        jar,
        headers,
        directory,
        filename,
        query.message,
        request,
    )
    .await
}

/// 原始 body 上传的公共实现：校验目标、流式落盘、复用 multipart 的入库逻辑。
async fn put_upload_inner(
    state: AppState,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    path: String,
    filename: String,
    message: Option<String>,
    request: axum::extract::Request,
) -> ApiResult<Json<serde_json::Value>> {
    let ctx = protected_request_context(&state, &jar).await?;

    if filename.is_empty() {
        return Err(ApiError::Domain(DomainError::Validation {
            message: "Missing filename: pass ?filename=... or include it in ?path=dir/name"
                .to_string(),
        }));
    }

    let max_upload_size = max_upload_size_bytes(&state);
    // Content-Length 已知时提前拒绝，避免白传一遍
    if let Some(length) = request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        && length > max_upload_size
    {
        return Err(ApiError::FileTooLarge {
            limit_bytes: max_upload_size,
            size_bytes: length,
        });
    }

    let temp_dir = state.config.storage.root.join("tmp");
    tokio::fs::create_dir_all(&temp_dir).await.map_err(|err| {
        ApiError::Internal(format!("Failed to create upload temp directory: {}", err))
    })?;
    let temp_path = temp_dir.join(format!("put-upload-{}.tmp", uuid::Uuid::new_v4()));

    // 流式写入临时文件，边写边校验上限
    let mut file_size: u64 = 0;
    let result: ApiResult<Json<serde_json::Value>> = async {
        let mut temp_file = tokio::fs::File::create(&temp_path).await.map_err(|err| {
            ApiError::Internal(format!("Failed to create upload temp file: {}", err))
        })?;

        use futures::StreamExt;
        let mut body = request.into_body().into_data_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|err| {
                ApiError::Domain(DomainError::Validation {
                    message: format!("Failed to read request body: {}", err),
                })
            })?;
            file_size = file_size.saturating_add(chunk.len() as u64);
            if file_size > max_upload_size {
                return Err(ApiError::FileTooLarge {
                    limit_bytes: max_upload_size,
                    size_bytes: file_size,
                });
            }
            temp_file.write_all(&chunk).await.map_err(|err| {
                ApiError::Internal(format!("Failed to write upload temp file: {}", err))
            })?;
        }
        temp_file.flush().await.map_err(|err| {
            ApiError::Internal(format!("Failed to flush upload temp file: {}", err))
        })?;

        let message = message
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Upload file");

        finish_single_upload(
            &state,
            &ctx,
            temp_path.as_std_path(),
            &path,
            &filename,
            message,
            file_size,
        )
        .await
    }
    .await;

    let _ = tokio::fs::remove_file(temp_path.as_std_path()).await;

    if let Ok(Json(payload)) = &result {
        let path = payload
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        crate::audit::record_for(
            &state,
            &headers,
            &ctx,
            NewAuditLog::success(crate::audit::action::FILE_UPLOAD)
                .target(path)
                .detail(format!("上传文件（{file_size} 字节）")),
        )
        .await;
    }

    result
}

async fn upload_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    multipart: Multipart,
) -> ApiResult<Json<serde_json::Value>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let temp_dir = state.config.storage.root.join("tmp");
    tokio::fs::create_dir_all(&temp_dir).await.map_err(|err| {
        ApiError::Internal(format!("Failed to create upload temp directory: {}", err))
    })?;
    let temp_path = temp_dir.join(format!("single-upload-{}.tmp", uuid::Uuid::new_v4()));

    let result = process_single_upload(&state, &ctx, multipart, temp_path.as_std_path()).await;
    let _ = tokio::fs::remove_file(temp_path.as_std_path()).await;

    // 成功时才记录（失败原因由错误处理链路返回给客户端）
    if let Ok(Json(payload)) = &result {
        let path = payload
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let size = payload
            .get("size")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        crate::audit::record_for(
            &state,
            &headers,
            &ctx,
            NewAuditLog::success(crate::audit::action::FILE_UPLOAD)
                .target(path)
                .detail(format!("上传文件（{size} 字节）")),
        )
        .await;
    }

    result
}

async fn process_single_upload(
    state: &AppState,
    ctx: &crate::routes::RequestContext,
    mut multipart: Multipart,
    temp_path: &std::path::Path,
) -> ApiResult<Json<serde_json::Value>> {
    tracing::info!("Processing streaming single file upload");

    let mut filename: Option<String> = None;
    let mut path: String = String::new();
    let mut message: String = "Upload file".to_string();
    let mut file_size: u64 = 0;
    let mut saw_file = false;
    let max_upload_size = max_upload_size_bytes(state);

    while let Some(mut field) = multipart.next_field().await.map_err(|err| {
        ApiError::Domain(DomainError::Validation {
            message: format!("Failed to read multipart field: {}", err),
        })
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                saw_file = true;
                filename = field.file_name().map(str::to_string);
                let mut temp_file = tokio::fs::File::create(temp_path).await.map_err(|err| {
                    ApiError::Internal(format!("Failed to create upload temp file: {}", err))
                })?;

                while let Some(chunk) = field.chunk().await.map_err(|err| {
                    ApiError::Domain(DomainError::Validation {
                        message: format!("Failed to read file data: {}", err),
                    })
                })? {
                    file_size = file_size.saturating_add(chunk.len() as u64);
                    if file_size > max_upload_size {
                        // 专用错误码 + 结构化上限，客户端可直接渲染「文件过大（上限 …）」
                        return Err(ApiError::FileTooLarge {
                            limit_bytes: max_upload_size,
                            size_bytes: file_size,
                        });
                    }
                    temp_file.write_all(&chunk).await.map_err(|err| {
                        ApiError::Internal(format!("Failed to write upload temp file: {}", err))
                    })?;
                }

                temp_file.flush().await.map_err(|err| {
                    ApiError::Internal(format!("Failed to flush upload temp file: {}", err))
                })?;
            }
            "path" => {
                let bytes = field.bytes().await.map_err(|_| {
                    ApiError::Domain(DomainError::Validation {
                        message: "Invalid path encoding".to_string(),
                    })
                })?;
                path = String::from_utf8(bytes.to_vec()).map_err(|_| {
                    ApiError::Domain(DomainError::Validation {
                        message: "Invalid path encoding".to_string(),
                    })
                })?;
            }
            "message" => {
                let bytes = field.bytes().await.map_err(|_| {
                    ApiError::Domain(DomainError::Validation {
                        message: "Invalid message encoding".to_string(),
                    })
                })?;
                message =
                    String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| "Upload file".to_string());
            }
            _ => {
                // Ignore unknown fields.
            }
        }
    }

    if !saw_file {
        return Err(ApiError::Domain(DomainError::Validation {
            message: "No file provided".to_string(),
        }));
    }

    let filename = filename.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "No filename provided".to_string(),
        })
    })?;

    finish_single_upload(state, ctx, temp_path, &path, &filename, &message, file_size).await
}

/// 校验文件名/路径并把临时文件入库（multipart 与原始 body 上传共用）。
#[allow(clippy::too_many_arguments)]
async fn finish_single_upload(
    state: &AppState,
    ctx: &crate::routes::RequestContext,
    temp_path: &std::path::Path,
    path: &str,
    filename: &str,
    message: &str,
    file_size: u64,
) -> ApiResult<Json<serde_json::Value>> {
    let filename = filename.to_string();

    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename == "."
        || filename == ".."
    {
        return Err(ApiError::Domain(DomainError::Validation {
            message: "Invalid filename".to_string(),
        }));
    }

    let target_path = NormalizedPath::new(path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    // 目标是已存在的目录时给出明确提示（否则会被当成同名冲突）
    let full_path = if target_path.as_str().is_empty() {
        filename.clone()
    } else {
        format!("{}/{filename}", target_path.as_str())
    };
    if let Ok(full) = NormalizedPath::new(&full_path)
        && let Ok(Some(existing)) = state
            .entry_repo
            .find_by_path(&ctx.namespace_id, &full)
            .await
        && matches!(existing.entry_type, vfiles_domain::EntryKind::Directory)
    {
        return Err(ApiError::Domain(DomainError::Validation {
            message: format!("Target path is a directory: {full_path}"),
        }));
    }

    let upload = state
        .upload_service
        .init_upload(
            &ctx.namespace_id,
            &target_path,
            &filename,
            file_size,
            None,
            Some(file_size.max(1)),
            &ctx.actor_user_id,
        )
        .await?;

    let temp_file = tokio::fs::File::open(temp_path)
        .await
        .map_err(|err| ApiError::Internal(format!("Failed to reopen upload temp file: {}", err)))?;

    let completed = state
        .upload_service
        .complete_upload_from_stream(&upload.upload_id, None, Some(message), Box::new(temp_file))
        .await?;

    tracing::info!(
        "Streaming single file upload completed: {} ({} bytes)",
        filename,
        file_size
    );

    Ok(Json(serde_json::json!({
        "upload_id": upload.upload_id.to_string(),
        "filename": filename,
        "size": file_size,
        "completed": true,
        "path": completed.entry.path_norm.as_str(),
        "version_id": completed.version.id.to_string(),
    })))
}
