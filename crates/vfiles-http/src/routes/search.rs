use axum::{
    extract::{Query, State},
    response::Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use vfiles_domain::{EntryKind, NormalizedPath, SearchQuery};

use crate::{AppState, dto::SearchResultDto, error::ApiError, routes::protected_request_context};

pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route("/search", axum::routing::get(search))
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: String,
    #[serde(default)]
    search_files: bool,
    #[serde(default)]
    search_content: bool,
    #[serde(default)]
    path: Option<String>,
    #[serde(default, rename = "type")]
    entry_type: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
}

fn default_limit() -> u32 {
    50
}

async fn search(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<SearchResultDto>>, ApiError> {
    let ctx = protected_request_context(&state, &jar).await?;
    if params.search_content && !state.config.features.search_content {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    // Validate query
    if params.q.trim().is_empty() {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Validation {
            message: "Search query cannot be empty".to_string(),
        }));
    }

    if params.q.len() > 100 {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Validation {
            message: "Search query too long".to_string(),
        }));
    }

    let path_prefix = match params
        .path
        .as_deref()
        .map(str::trim)
        .map(|value| value.trim_matches('/'))
        .filter(|value| !value.is_empty())
    {
        Some(path) => Some(NormalizedPath::new(path).map_err(|message| {
            ApiError::Domain(vfiles_domain::DomainError::Validation { message })
        })?),
        None => None,
    };

    let entry_kind = match params.entry_type.as_deref().map(str::trim) {
        Some("file") => Some(EntryKind::File),
        Some("directory") => Some(EntryKind::Directory),
        Some("all") | Some("") | None => None,
        Some(_) => {
            return Err(ApiError::Domain(vfiles_domain::DomainError::Validation {
                message: "Invalid search type".to_string(),
            }));
        }
    };

    // Build search query
    let query = SearchQuery {
        query: params.q,
        namespace_id: ctx.namespace_id,
        search_files: params.search_files || (!params.search_files && !params.search_content), // Default to files if neither specified
        search_content: params.search_content,
        path_prefix,
        entry_kind,
        limit: params.limit.min(500),
        offset: params.offset,
    };

    // Perform search
    let results = state.search_service.search(query).await?;

    // Convert to DTOs
    let dtos = results.into_iter().map(SearchResultDto::from).collect();

    Ok(Json(dtos))
}
