//! Share routes for file sharing functionality.

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Json, Response},
    routing::{delete, get, post},
};

use crate::{
    AppState,
    dto::{CreateShareRequest, CreateShareResponse, ShareDto},
    error::ApiError,
    http_headers::attachment_header,
    routes::authenticated_request_context,
};
use axum_extra::extract::cookie::CookieJar;
use tokio_util::io::ReaderStream;
use vfiles_domain::{DomainError, EntryKind};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/shares", post(create_share))
        .route("/shares", get(list_shares))
        .route("/shares/{code}/download", get(download_share))
        .route("/shares/{code}", get(access_share))
        .route("/shares/{code}", delete(disable_share))
}

fn build_download_response(
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    filename: &str,
    mime_type: Option<&str>,
    size_bytes: u64,
) -> Result<Response, ApiError> {
    let mut response = Response::new(axum::body::Body::from_stream(ReaderStream::new(reader)));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(mime_type.unwrap_or("application/octet-stream"))
            .map_err(|e| ApiError::Internal(format!("Invalid content type header: {}", e)))?,
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        attachment_header(filename)?,
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_LENGTH,
        axum::http::HeaderValue::from_str(&size_bytes.to_string())
            .map_err(|e| ApiError::Internal(format!("Invalid content length header: {}", e)))?,
    );

    Ok(response)
}

fn build_archive_response(filename: &str, bytes: Vec<u8>) -> Result<Response, ApiError> {
    let content_length = bytes.len();
    let mut response = Response::new(axum::body::Body::from(bytes));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/zip"),
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        attachment_header(filename)?,
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_LENGTH,
        axum::http::HeaderValue::from_str(&content_length.to_string())
            .map_err(|e| ApiError::Internal(format!("Invalid content length header: {}", e)))?,
    );

    Ok(response)
}

async fn create_share(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateShareRequest>,
) -> Result<Json<CreateShareResponse>, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    let ctx = authenticated_request_context(&state, &jar).await?;
    let user_id = ctx.actor_user_id;

    tracing::info!("Creating share for path: {}", req.path);

    // Parse expiration if provided
    let expires_at = if let Some(expires_str) = req.expires_at {
        Some(
            time::OffsetDateTime::parse(
                &expires_str,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| {
                ApiError::Domain(vfiles_domain::DomainError::Validation {
                    message: "Invalid datetime format, expected RFC3339".to_string(),
                })
            })?,
        )
    } else {
        None
    };

    // Parse path
    let entry_path = vfiles_domain::NormalizedPath::new(&req.path).map_err(|_| {
        ApiError::Domain(vfiles_domain::DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    // Create the share
    let code = state
        .share_service
        .create_share(&ctx.namespace_id, &entry_path, expires_at, &user_id)
        .await?;

    let mut share_url = state.config.http.public_base_url.clone();
    share_url.set_path(&format!("/s/{}", code));
    share_url.set_query(None);

    let response = CreateShareResponse {
        code,
        share_url: share_url.to_string(),
    };

    tracing::info!("Share created with code: {}", response.code);

    Ok(Json(response))
}

async fn list_shares(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<ShareDto>>, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    let ctx = authenticated_request_context(&state, &jar).await?;
    let user_id = ctx.actor_user_id;

    tracing::info!("Listing shares for user: {}", user_id.to_string());

    let shares = state.share_service.list_shares_by_user(&user_id).await?;

    let dtos = shares.into_iter().map(ShareDto::from).collect();

    Ok(Json(dtos))
}

async fn access_share(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<ShareDto>, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    tracing::info!("Accessing share with code: {}", code);

    let share = state.share_service.access_share(&code).await?;

    let dto = ShareDto::from(share);

    Ok(Json(dto))
}

pub async fn download_share(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Response, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }

    let share = state.share_service.access_share(&code).await?;
    let entry = state.entry_repo.find_by_id(&share.entry_id).await?;

    match entry.entry_type {
        EntryKind::File => {
            let version = share.entry_version_id.map(|value| value.to_string());
            let file = state
                .workspace_service
                .open_file(&share.namespace_id, &entry.path_norm, version.as_deref())
                .await?;

            build_download_response(
                file.reader,
                &file.filename,
                file.mime_type.as_deref(),
                file.size_bytes,
            )
        }
        EntryKind::Directory => {
            let archive = state
                .workspace_service
                .download_directory_archive(&share.namespace_id, &entry.path_norm, None)
                .await?;

            build_archive_response(&archive.filename, archive.bytes)
        }
    }
}

async fn disable_share(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(code): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    let ctx = authenticated_request_context(&state, &jar).await?;
    let user_id = ctx.actor_user_id;

    tracing::info!(
        "Disabling share with code: {} for user: {}",
        code,
        user_id.to_string()
    );

    state.share_service.disable_share(&code, &user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
