use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{
        HeaderValue,
        header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE},
    },
    response::Response,
    routing::get,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use tokio_util::io::ReaderStream;

use crate::{
    AppState,
    error::{ApiError, ApiResult},
    http_headers::attachment_header,
    routes::protected_request_context,
};
use vfiles_domain::DomainError;

#[derive(Debug, Deserialize)]
struct DownloadQuery {
    path: Option<String>,
    commit: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(download_file))
        .route("/folder", get(download_folder))
}

fn build_download_response(
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    filename: &str,
    mime_type: Option<&str>,
    size_bytes: u64,
) -> ApiResult<Response> {
    let mut response = Response::new(Body::from_stream(ReaderStream::new(reader)));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(mime_type.unwrap_or("application/octet-stream"))
            .map_err(|e| ApiError::Internal(format!("Invalid content type header: {}", e)))?,
    );
    response
        .headers_mut()
        .insert(CONTENT_DISPOSITION, attachment_header(filename)?);
    response.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&size_bytes.to_string())
            .map_err(|e| ApiError::Internal(format!("Invalid content length header: {}", e)))?,
    );

    Ok(response)
}

fn build_archive_response(filename: &str, bytes: Vec<u8>) -> ApiResult<Response> {
    let content_length = bytes.len();
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    response
        .headers_mut()
        .insert(CONTENT_DISPOSITION, attachment_header(filename)?);
    response.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string())
            .map_err(|e| ApiError::Internal(format!("Invalid content length header: {}", e)))?,
    );

    Ok(response)
}

async fn download_file(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<DownloadQuery>,
) -> ApiResult<Response> {
    let ctx = protected_request_context(&state, &jar).await?;
    let raw_path = query.path.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "Missing path parameter".to_string(),
        })
    })?;
    let path = vfiles_domain::NormalizedPath::new(&raw_path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    let file = state
        .workspace_service
        .open_file(&ctx.namespace_id, &path, query.commit.as_deref())
        .await?;

    build_download_response(
        file.reader,
        &file.filename,
        file.mime_type.as_deref(),
        file.size_bytes,
    )
}

async fn download_folder(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<DownloadQuery>,
) -> ApiResult<Response> {
    let ctx = protected_request_context(&state, &jar).await?;
    let raw_path = query.path.unwrap_or_default();
    let path = vfiles_domain::NormalizedPath::new(&raw_path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;
    let archive = state
        .workspace_service
        .download_directory_archive(&ctx.namespace_id, &path, query.commit.as_deref())
        .await?;

    build_archive_response(&archive.filename, archive.bytes)
}
