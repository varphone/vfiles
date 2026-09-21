//! File tree browsing routes.

use axum::{
    Json, Router,
    extract::Query,
    http::StatusCode,
    routing::{delete, get, post},
};
use axum_extra::extract::cookie::CookieJar;

use crate::{
    AppState,
    dto::{CreateDirectoryRequest, EntryDto, EntryPageDto, MoveEntryRequest},
    error::{ApiError, ApiJson, ApiResult},
    routes::protected_request_context,
};
use vfiles_domain::{DomainError, NamespaceId, NewAuditLog, NormalizedPath, SnapshotId};

#[derive(Debug, Default, serde::Deserialize)]
struct TreeQuery {
    commit: Option<String>,
}

/// 分页参数：默认每页 200，最多 1000。
#[derive(Debug, Default, serde::Deserialize)]
struct TreePageQuery {
    commit: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

const DEFAULT_PAGE_LIMIT: usize = 200;
const MAX_PAGE_LIMIT: usize = 1000;

#[derive(Debug, Default, serde::Deserialize)]
struct DeleteQuery {
    path: Option<String>,
    message: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", delete(delete_entry))
        .route("/move", post(move_entry))
        .route("/tree", get(list_root))
        .route("/tree/{*path}", get(list_directory))
        .route("/list", get(list_root_page))
        .route("/list/{*path}", get(list_directory_page))
        .route("/directories", post(create_directory))
}

pub async fn move_entry(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    ApiJson(req): ApiJson<MoveEntryRequest>,
) -> ApiResult<StatusCode> {
    let ctx = protected_request_context(&state, &jar).await?;
    let source_path = NormalizedPath::new(&req.from).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid source path format".to_string(),
        })
    })?;
    let destination_path = NormalizedPath::new(&req.to).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid destination path format".to_string(),
        })
    })?;

    if source_path.as_str().is_empty() || destination_path.as_str().is_empty() {
        return Err(ApiError::Domain(DomainError::Validation {
            message: "Source and destination paths are required".to_string(),
        }));
    }

    state
        .workspace_service
        .move_entries(
            &ctx.namespace_id,
            std::slice::from_ref(&source_path),
            &destination_path,
            req.message.as_deref(),
            &ctx.actor_user_id,
        )
        .await?;

    crate::audit::record_for(
        &state,
        &headers,
        &ctx,
        NewAuditLog::success(crate::audit::action::FILE_MOVE)
            .target(format!(
                "{} → {}",
                source_path.as_str(),
                destination_path.as_str()
            ))
            .detail("移动条目"),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_entry(
    Query(query): Query<DeleteQuery>,
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
) -> ApiResult<StatusCode> {
    let ctx = protected_request_context(&state, &jar).await?;
    let raw_path = query.path.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "Missing path parameter".to_string(),
        })
    })?;
    let normalized_path = NormalizedPath::new(&raw_path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    if normalized_path.as_str().is_empty() {
        return Err(ApiError::Domain(DomainError::Validation {
            message: "Cannot delete the root directory".to_string(),
        }));
    }

    state
        .workspace_service
        .delete_entries(
            &ctx.namespace_id,
            std::slice::from_ref(&normalized_path),
            query.message.as_deref(),
            &ctx.actor_user_id,
        )
        .await?;

    crate::audit::record_for(
        &state,
        &headers,
        &ctx,
        NewAuditLog::success(crate::audit::action::FILE_DELETE)
            .target(normalized_path.as_str())
            .detail("删除条目"),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_root(
    Query(query): Query<TreeQuery>,
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<Vec<EntryDto>>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let root_path = NormalizedPath::new("").map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid root path".to_string(),
        })
    })?;

    list_directory_impl(
        &state,
        &ctx.namespace_id,
        &root_path,
        query.commit.as_deref(),
    )
    .await
}

async fn list_directory(
    axum::extract::Path(path): axum::extract::Path<String>,
    Query(query): Query<TreeQuery>,
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<Vec<EntryDto>>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let normalized_path = NormalizedPath::new(&path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    list_directory_impl(
        &state,
        &ctx.namespace_id,
        &normalized_path,
        query.commit.as_deref(),
    )
    .await
}

fn parse_snapshot_id(raw: Option<&str>) -> ApiResult<Option<SnapshotId>> {
    raw.filter(|value| !value.trim().is_empty())
        .map(|value| {
            SnapshotId::from_string(value).map_err(|_| {
                ApiError::Domain(DomainError::Validation {
                    message: "Invalid commit/snapshot ID format".to_string(),
                })
            })
        })
        .transpose()
}

async fn list_directory_impl(
    state: &AppState,
    namespace_id: &NamespaceId,
    path: &NormalizedPath,
    commit: Option<&str>,
) -> ApiResult<Json<Vec<EntryDto>>> {
    let snapshot_id = parse_snapshot_id(commit)?;
    let tree = state
        .workspace_service
        .tree(namespace_id, path, snapshot_id.as_ref())
        .await?;

    Ok(Json(tree.items.into_iter().map(Into::into).collect()))
}

/// 分页列出根目录：`GET /api/files/list?limit=&offset=&commit=`
async fn list_root_page(
    Query(query): Query<TreePageQuery>,
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<EntryPageDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let root_path = NormalizedPath::new("").map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid root path".to_string(),
        })
    })?;

    paginated_listing(&state, &ctx.namespace_id, &root_path, &query).await
}

/// 分页列出目录：`GET /api/files/list/{path}?limit=&offset=&commit=`
async fn list_directory_page(
    axum::extract::Path(path): axum::extract::Path<String>,
    Query(query): Query<TreePageQuery>,
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<EntryPageDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let normalized_path = NormalizedPath::new(&path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    paginated_listing(&state, &ctx.namespace_id, &normalized_path, &query).await
}

/// 分页列目录：HEAD 走 SQL 分页（大目录不再全量拉取），
/// 指定快照时仍按快照重建整棵子树后切片。
async fn paginated_listing(
    state: &AppState,
    namespace_id: &NamespaceId,
    path: &NormalizedPath,
    query: &TreePageQuery,
) -> ApiResult<Json<EntryPageDto>> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    let offset = query.offset.unwrap_or(0);

    if let Some(commit) = query.commit.as_deref() {
        let items = fetch_entry_items(state, namespace_id, path, Some(commit)).await?;
        return Ok(Json(paginate(items, Some(limit), Some(offset))));
    }

    let (items, total) = state
        .workspace_service
        .live_children_page(namespace_id, path, limit as u32, offset as u32)
        .await?;

    let items: Vec<EntryDto> = items.into_iter().map(Into::into).collect();
    let total = total as usize;
    let offset = offset.min(total);
    let end = (offset + items.len()).min(total);

    Ok(Json(EntryPageDto {
        items,
        total,
        limit,
        offset,
        has_more: end < total,
    }))
}

async fn fetch_entry_items(
    state: &AppState,
    namespace_id: &NamespaceId,
    path: &NormalizedPath,
    commit: Option<&str>,
) -> ApiResult<Vec<EntryDto>> {
    let snapshot_id = parse_snapshot_id(commit)?;
    let tree = state
        .workspace_service
        .tree(namespace_id, path, snapshot_id.as_ref())
        .await?;

    Ok(tree.items.into_iter().map(Into::into).collect())
}

/// 对完整列表分页；`total` 始终为全量条目数，便于客户端展示与判断是否还有更多。
fn paginate(items: Vec<EntryDto>, limit: Option<usize>, offset: Option<usize>) -> EntryPageDto {
    let total = items.len();
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
    let offset = offset.unwrap_or(0).min(total);
    let end = (offset + limit).min(total);

    let page = items
        .into_iter()
        .skip(offset)
        .take(end - offset)
        .collect::<Vec<_>>();

    EntryPageDto {
        items: page,
        total,
        limit,
        offset,
        has_more: end < total,
    }
}

pub async fn create_directory(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    ApiJson(req): ApiJson<CreateDirectoryRequest>,
) -> ApiResult<Json<EntryDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let normalized_path = NormalizedPath::new(&req.path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    state
        .workspace_service
        .create_directory(
            &ctx.namespace_id,
            &normalized_path,
            None,
            &ctx.actor_user_id,
        )
        .await?;

    let parent_path = normalized_path
        .as_str()
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("");
    let parent_path = NormalizedPath::new(parent_path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid parent path format".to_string(),
        })
    })?;

    let tree = state
        .workspace_service
        .tree(&ctx.namespace_id, &parent_path, None)
        .await?
        .items;

    let entry = tree
        .into_iter()
        .find(|item| item.path == normalized_path.as_str())
        .ok_or(ApiError::Domain(DomainError::NotFound {
            resource: "entry".to_string(),
        }))?;

    let dto: EntryDto = entry.into();
    Ok(Json(dto))
}
