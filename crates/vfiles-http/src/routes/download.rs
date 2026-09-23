use crate::{
    AppState,
    error::{ApiError, ApiResult},
    http_headers::{StreamingFileOptions, streaming_file_response},
    routes::protected_request_context,
};
use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, Method},
    response::Response,
    routing::get,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use vfiles_domain::{DomainError, NewAuditLog};

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

async fn download_file(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
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

    crate::audit::record_for(
        &state,
        &headers,
        &ctx,
        NewAuditLog::success(crate::audit::action::FILE_DOWNLOAD)
            .target(path.as_str())
            .detail("下载文件"),
    )
    .await;

    streaming_file_response(
        file.reader,
        StreamingFileOptions {
            range_allowed: method == Method::GET,
            request_headers: &headers,
            mime_type: file.mime_type.as_deref(),
            size_bytes: file.size_bytes,
            attachment_filename: Some(&file.filename),
            etag: Some(&file.etag),
            modified_at: file.modified_at,
        },
    )
    .await
}

async fn download_folder(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
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

    crate::audit::record_for(
        &state,
        &headers,
        &ctx,
        NewAuditLog::success(crate::audit::action::FILE_DOWNLOAD)
            .target(path.as_str())
            .detail("下载目录（打包）"),
    )
    .await;

    streaming_file_response(
        archive.reader,
        StreamingFileOptions {
            range_allowed: method == Method::GET,
            request_headers: &headers,
            mime_type: Some("application/zip"),
            size_bytes: archive.size_bytes,
            attachment_filename: Some(&archive.filename),
            etag: None,
            modified_at: None,
        },
    )
    .await
}
