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
use vfiles_domain::DomainError;

#[derive(Debug, Deserialize)]
struct ContentQuery {
    path: Option<String>,
    commit: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/content", get(get_file_content))
}

async fn get_file_content(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    jar: CookieJar,
    Query(query): Query<ContentQuery>,
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

    streaming_file_response(
        file.reader,
        StreamingFileOptions {
            range_allowed: method == Method::GET,
            request_headers: &headers,
            mime_type: file.mime_type.as_deref(),
            size_bytes: file.size_bytes,
            attachment_filename: None,
            etag: Some(&file.etag),
            modified_at: file.modified_at,
        },
    )
    .await
}
