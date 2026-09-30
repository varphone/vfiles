//! 收藏夹：把常用条目固定在侧栏。

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use vfiles_domain::{DomainError, EntryId, NamespaceId, NormalizedPath};

use crate::{
    AppState,
    dto::EntryDto,
    error::{ApiError, ApiJson, ApiResult},
    routes::protected_request_context,
};

#[derive(Debug, Serialize)]
pub struct FavoriteDto {
    pub path: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct FavoriteListDto {
    pub items: Vec<FavoriteDto>,
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct FavoritePageQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 200;

#[derive(Debug, Deserialize)]
pub struct FavoriteBody {
    pub path: String,
}

/// DELETE 走查询参数：`DELETE` 请求体在部分客户端/代理上不可靠。
#[derive(Debug, Deserialize)]
pub struct FavoriteQuery {
    pub path: String,
}

pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/favorites",
        axum::routing::get(list).post(add).delete(remove),
    )
}

/// 查询一批条目的收藏 ID，目录和搜索列表用它批量标记收藏状态。
pub(crate) async fn favorite_ids(
    state: &AppState,
    namespace_id: &NamespaceId,
    entry_ids: &[EntryId],
) -> ApiResult<HashSet<EntryId>> {
    Ok(state
        .favorite_repo
        .contains_many(namespace_id, entry_ids)
        .await?)
}

/// 为有限大小的文件列表填充收藏状态，避免逐条查询。
pub(crate) async fn mark_favorite_status(
    state: &AppState,
    namespace_id: &NamespaceId,
    items: &mut [EntryDto],
) -> ApiResult<()> {
    let entry_ids = items
        .iter()
        .map(|item| {
            EntryId::from_string(&item.id).map_err(|error| {
                ApiError::Domain(DomainError::Internal {
                    message: format!("Invalid entry ID in file listing: {error}"),
                })
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let favorite_ids = favorite_ids(state, namespace_id, &entry_ids).await?;
    for (item, entry_id) in items.iter_mut().zip(entry_ids) {
        item.is_favorite = Some(favorite_ids.contains(&entry_id));
    }
    Ok(())
}

fn to_dto(entry: vfiles_domain::Entry) -> FavoriteDto {
    let path = entry.path_norm.as_str().to_string();
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    FavoriteDto {
        path,
        name,
        kind: match entry.entry_type {
            vfiles_domain::EntryKind::Directory => "directory".to_string(),
            vfiles_domain::EntryKind::File => "file".to_string(),
        },
    }
}

/// 收藏列表（按收藏时间倒序）。
pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<FavoritePageQuery>,
) -> ApiResult<Json<FavoriteListDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    let offset = u32::try_from(query.offset.unwrap_or(0)).unwrap_or(u32::MAX);
    let (entries, total) = state
        .favorite_repo
        .list_page(&ctx.namespace_id, limit as u32, offset)
        .await?;
    let item_count = entries.len() as u64;
    let response_offset = u64::from(offset).min(total);

    Ok(Json(FavoriteListDto {
        items: entries.into_iter().map(to_dto).collect(),
        total,
        limit,
        offset: usize::try_from(response_offset).unwrap_or(usize::MAX),
        has_more: response_offset.saturating_add(item_count) < total,
    }))
}

/// 添加收藏（幂等）。
pub async fn add(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiJson(body): ApiJson<FavoriteBody>,
) -> ApiResult<(StatusCode, Json<FavoriteListDto>)> {
    let ctx = protected_request_context(&state, &jar).await?;
    let entry_id = resolve_entry(&state, &ctx.namespace_id, &body.path).await?;

    state
        .favorite_repo
        .add(&ctx.namespace_id, &entry_id)
        .await?;

    let (entries, total) = state
        .favorite_repo
        .list_page(&ctx.namespace_id, DEFAULT_PAGE_LIMIT as u32, 0)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(FavoriteListDto {
            items: entries.into_iter().map(to_dto).collect(),
            total,
            limit: DEFAULT_PAGE_LIMIT,
            offset: 0,
            has_more: total > DEFAULT_PAGE_LIMIT as u64,
        }),
    ))
}

/// 取消收藏（幂等）。
pub async fn remove(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<FavoriteQuery>,
) -> ApiResult<Json<FavoriteListDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let entry_id = resolve_entry(&state, &ctx.namespace_id, &query.path).await?;

    state
        .favorite_repo
        .remove(&ctx.namespace_id, &entry_id)
        .await?;

    let (entries, total) = state
        .favorite_repo
        .list_page(&ctx.namespace_id, DEFAULT_PAGE_LIMIT as u32, 0)
        .await?;
    Ok(Json(FavoriteListDto {
        items: entries.into_iter().map(to_dto).collect(),
        total,
        limit: DEFAULT_PAGE_LIMIT,
        offset: 0,
        has_more: total > DEFAULT_PAGE_LIMIT as u64,
    }))
}

/// 把请求里的路径解析为条目 ID；路径不存在时报 404 而不是静默忽略。
async fn resolve_entry(
    state: &AppState,
    namespace_id: &vfiles_domain::NamespaceId,
    raw_path: &str,
) -> ApiResult<vfiles_domain::EntryId> {
    let normalized = NormalizedPath::new(raw_path.trim_matches('/')).map_err(|message| {
        ApiError::Domain(DomainError::Validation {
            message: format!("Invalid path: {message}"),
        })
    })?;

    let entry = state
        .entry_repo
        .find_by_path(namespace_id, &normalized)
        .await?
        .ok_or_else(|| {
            ApiError::Domain(DomainError::NotFound {
                resource: "entry".to_string(),
            })
        })?;

    Ok(entry.id)
}
