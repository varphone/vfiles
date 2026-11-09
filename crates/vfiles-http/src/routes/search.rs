use axum::{extract::State, response::Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use vfiles_domain::{EntryKind, NormalizedPath, SearchQuery, SearchResult, SessionScope};

use crate::{
    AppState,
    dto::{SearchPageDto, SearchResultDto},
    error::{ApiError, ApiQuery},
    routes::protected_request_context,
};

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

/// 单页上限：客户端按需翻页，避免一次拉回过多结果。
const MAX_SEARCH_LIMIT: u32 = 500;

async fn search(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiQuery(params): ApiQuery<SearchParams>,
) -> Result<Json<SearchPageDto>, ApiError> {
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

    let limit = params.limit.clamp(1, MAX_SEARCH_LIMIT);
    let search_files = params.search_files || (!params.search_files && !params.search_content);
    let search_content = params.search_content;
    let entry_kind = entry_kind;
    let query_for =
        |path_prefix: Option<NormalizedPath>, offset: u32, page_limit: u32| SearchQuery {
            query: params.q.clone(),
            namespace_id: ctx.namespace_id,
            search_files,
            search_content,
            path_prefix,
            entry_kind,
            limit: page_limit,
            offset,
        };

    // 受限会话：显式目录必须在范围内；未指定目录时逐根搜索后合并，结果不会越界。
    let (results, has_more, results_may_be_incomplete) = match &ctx.scope {
        SessionScope::Full => {
            let mut page = state
                .search_service
                .search_page(query_for(path_prefix, params.offset, limit + 1))
                .await?;
            let has_more = page.items.len() > limit as usize;
            page.items.truncate(limit as usize);
            (page.items, has_more, page.results_may_be_incomplete)
        }
        SessionScope::Restricted(scope) => {
            if let Some(prefix) = path_prefix.as_ref() {
                ctx.require_read(prefix.as_str())?;
                let mut page = state
                    .search_service
                    .search_page(query_for(Some(prefix.clone()), params.offset, limit + 1))
                    .await?;
                let has_more = page.items.len() > limit as usize;
                page.items.truncate(limit as usize);
                (page.items, has_more, page.results_may_be_incomplete)
            } else {
                let per_root_limit =
                    (params.offset.saturating_add(limit).saturating_add(1)).min(MAX_SEARCH_LIMIT);
                let mut merged: Vec<SearchResult> = Vec::new();
                let mut seen = std::collections::HashSet::new();
                let mut incomplete = false;
                for root in &scope.roots {
                    let page = state
                        .search_service
                        .search_page(query_for(Some(root.clone()), 0, per_root_limit))
                        .await?;
                    incomplete |= page.results_may_be_incomplete;
                    for item in page.items {
                        if seen.insert(item.entry.id) {
                            merged.push(item);
                        }
                    }
                }
                merged.sort_by(|left, right| {
                    right
                        .score
                        .partial_cmp(&left.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            left.entry
                                .path_norm
                                .as_str()
                                .cmp(right.entry.path_norm.as_str())
                        })
                });
                let start = (params.offset as usize).min(merged.len());
                let end = start.saturating_add(limit as usize).min(merged.len());
                let has_more = merged.len() > end;
                (merged[start..end].to_vec(), has_more, incomplete)
            }
        }
    };

    let entry_ids: Vec<_> = results.iter().map(|result| result.entry.id).collect();
    let favorite_ids =
        super::favorites::favorite_ids(&state, &ctx.namespace_id, &entry_ids).await?;
    let items = results
        .into_iter()
        .map(|result| {
            let entry_id = result.entry.id;
            let mut dto = SearchResultDto::from(result);
            dto.entry.is_favorite = Some(favorite_ids.contains(&entry_id));
            dto
        })
        .collect();

    Ok(Json(SearchPageDto {
        items,
        limit,
        offset: params.offset,
        has_more,
        results_may_be_incomplete,
    }))
}
