use crate::{
    AppState,
    error::{ApiError, ApiResult},
    http_headers::streaming_file_response,
    routes::protected_request_context,
};
use axum::{
    Router,
    extract::{Query, State},
    http::HeaderMap,
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
        &headers,
        file.mime_type.as_deref(),
        file.size_bytes,
        None,
        Some(&file.etag),
        file.modified_at,
    )
    .await
}
