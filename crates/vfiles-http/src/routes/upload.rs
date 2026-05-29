//! File upload routes.

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    routing::{post, put},
};
use axum_extra::extract::{Multipart, cookie::CookieJar};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    AppState,
    dto::UploadRequest,
    error::{ApiError, ApiResult},
    routes::protected_request_context,
};
use vfiles_domain::{DomainError, NormalizedPath, UploadId};

#[derive(Debug, Deserialize)]
struct CompleteUploadRequest {
    message: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/upload", post(upload_file))
        .route("/upload/init", post(create_upload))
        .route(
            "/upload/chunks/{upload_id}/{chunk_index}",
            put(upload_chunk),
        )
        .route("/upload/complete/{upload_id}", post(complete_upload))
        .layer(DefaultBodyLimit::disable())
}

fn max_upload_size_bytes(state: &AppState) -> u64 {
    state.config.limits.max_upload_size_bytes
}

async fn create_upload(
    jar: CookieJar,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<UploadRequest>,
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

    tracing::info!("Completing upload session: {}", upload_id);
    let message = req.and_then(|Json(body)| body.message);
    let completed = state
        .upload_service
        .complete_upload(&upload_id, None, message.as_deref())
        .await?;
    tracing::info!("Upload session {} completed successfully", upload_id);

    Ok(Json(serde_json::json!({
        "upload_id": upload_id.to_string(),
        "completed": true,
        "path": completed.entry.path_norm.as_str(),
        "version_id": completed.version.id.to_string(),
    })))
}

async fn upload_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    mut multipart: Multipart,
) -> ApiResult<Json<serde_json::Value>> {
    let ctx = protected_request_context(&state, &jar).await?;
    tracing::info!("Processing single file upload");

    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut path: String = String::new();
    let mut message: String = "Upload file".to_string();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError::Domain(DomainError::Validation {
            message: format!("Failed to read multipart field: {}", e),
        })
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                let data = field.bytes().await.map_err(|e| {
                    ApiError::Domain(DomainError::Validation {
                        message: format!("Failed to read file data: {}", e),
                    })
                })?;
                file_data = Some(data.to_vec());
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
                // Ignore unknown fields
            }
        }
    }

    let file_data = file_data.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "No file provided".to_string(),
        })
    })?;

    let filename = filename.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "No filename provided".to_string(),
        })
    })?;

    // Validate filename
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

    // Validate file size
    let max_upload_size = max_upload_size_bytes(&state);
    if u64::try_from(file_data.len()).expect("multipart payload length should fit in u64")
        > max_upload_size
    {
        return Err(ApiError::Domain(DomainError::Validation {
            message: format!("File too large. Maximum size is {} bytes", max_upload_size),
        }));
    }

    let target_path = NormalizedPath::new(&path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    let upload = state
        .upload_service
        .init_upload(
            &ctx.namespace_id,
            &target_path,
            &filename,
            file_data.len() as u64,
            None,
            Some(file_data.len() as u64),
            &ctx.actor_user_id,
        )
        .await?;

    let mut hasher = Sha256::new();
    hasher.update(&file_data);
    let sha256 = hex::encode(hasher.finalize());

    state
        .upload_service
        .upload_part(&upload.upload_id, 0, &file_data)
        .await?;

    let completed = state
        .upload_service
        .complete_upload(&upload.upload_id, Some(&sha256), Some(message.as_str()))
        .await?;

    tracing::info!(
        "Single file upload completed: {} ({} bytes)",
        filename,
        file_data.len()
    );

    Ok(Json(serde_json::json!({
        "upload_id": upload.upload_id.to_string(),
        "filename": filename,
        "size": file_data.len(),
        "completed": true,
        "path": completed.entry.path_norm.as_str(),
        "version_id": completed.version.id.to_string(),
    })))
}
