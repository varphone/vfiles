//! 收藏夹：把常用条目固定在侧栏。

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use vfiles_domain::{DomainError, NormalizedPath};

use crate::{
    AppState,
    error::{ApiError, ApiResult},
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
}

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
) -> ApiResult<Json<FavoriteListDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let entries = state.favorite_repo.list(&ctx.namespace_id).await?;

    Ok(Json(FavoriteListDto {
        items: entries.into_iter().map(to_dto).collect(),
    }))
}

/// 添加收藏（幂等）。
pub async fn add(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<FavoriteBody>,
) -> ApiResult<(StatusCode, Json<FavoriteListDto>)> {
    let ctx = protected_request_context(&state, &jar).await?;
    let entry_id = resolve_entry(&state, &ctx.namespace_id, &body.path).await?;

    state
        .favorite_repo
        .add(&ctx.namespace_id, &entry_id)
        .await?;

    let entries = state.favorite_repo.list(&ctx.namespace_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(FavoriteListDto {
            items: entries.into_iter().map(to_dto).collect(),
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

    let entries = state.favorite_repo.list(&ctx.namespace_id).await?;
    Ok(Json(FavoriteListDto {
        items: entries.into_iter().map(to_dto).collect(),
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
