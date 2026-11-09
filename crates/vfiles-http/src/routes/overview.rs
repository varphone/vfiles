//! 工作区聚合统计：侧栏「存储用量 / 最近文件」的数据来源。

use axum::{extract::State, response::Json, routing::get};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use vfiles_domain::Entry;

use crate::{
    AppState,
    error::{ApiQuery, ApiResult},
    routes::protected_request_context,
};

/// 概览接口附带的最新文件数量（侧栏只展示少量）。
const RECENT_FILE_LIMIT: u32 = 8;
/// `/api/files/recent` 的默认条数与上限。
const DEFAULT_RECENT_LIMIT: u32 = 50;
const MAX_RECENT_LIMIT: u32 = 200;

#[derive(Debug, Serialize)]
pub struct RecentFileDto {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct RecentFilesQuery {
    limit: Option<u32>,
}

/// 按类型聚合的占用（侧栏占比条使用）。
#[derive(Debug, Serialize)]
pub struct CategoryUsageDto {
    /// `document` / `image` / `video` / `audio` / `other`
    pub category: String,
    pub bytes: u64,
    pub file_count: u64,
}

#[derive(Debug, Serialize)]
pub struct OverviewDto {
    pub file_count: u64,
    pub directory_count: u64,
    pub total_bytes: u64,
    /// 按类型聚合的占用，按字节数倒序；空命名空间为空数组。
    pub categories: Vec<CategoryUsageDto>,
    pub recent_files: Vec<RecentFileDto>,
}

pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route("/overview", get(overview))
}

pub async fn overview(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<OverviewDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    // 整命名空间的存储统计属于账号级信息，受限密码不展示。
    ctx.require_full()?;

    // 统计、分类与最近更新来自同一遍扫描，避免为每一项重新遍历命名空间。
    let overview = state
        .entry_repo
        .namespace_overview(&ctx.namespace_id, RECENT_FILE_LIMIT)
        .await?;
    let (stats, categories) = (overview.stats, overview.categories);
    let recent_files = recent_file_dtos(&state, overview.recent_files).await;

    Ok(Json(OverviewDto {
        file_count: stats.file_count,
        directory_count: stats.directory_count,
        total_bytes: stats.total_bytes,
        categories: categories
            .into_iter()
            .map(|usage| CategoryUsageDto {
                category: usage.category.as_str().to_string(),
                bytes: usage.bytes,
                file_count: usage.file_count,
            })
            .collect(),
        recent_files,
    }))
}

/// 按 `updated_at` 倒序返回当前命名空间最近修改的文件。
///
/// 受限会话只保留 `allows_path_read` 的路径：范围过滤在仓储返回之后进行，
/// 所以先按上限多取一些，避免过滤后不足调用方请求的条数。
pub async fn recent(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiQuery(query): ApiQuery<RecentFilesQuery>,
) -> ApiResult<Json<Vec<RecentFileDto>>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_RECENT_LIMIT)
        .clamp(1, MAX_RECENT_LIMIT);
    let fetch_limit = if ctx.scope.is_full() {
        limit
    } else {
        MAX_RECENT_LIMIT
    };

    let overview = state
        .entry_repo
        .namespace_overview(&ctx.namespace_id, fetch_limit)
        .await?;
    let mut files = recent_file_dtos(&state, overview.recent_files).await;
    if !ctx.scope.is_full() {
        files.retain(|file| ctx.scope.allows_path_read(&file.path));
    }
    files.truncate(limit as usize);

    Ok(Json(files))
}

/// 最近文件需要展示大小与类型：按 entry 的当前版本补齐。
async fn recent_file_dtos(state: &AppState, recent: Vec<Entry>) -> Vec<RecentFileDto> {
    let version_ids: Vec<_> = recent
        .iter()
        .filter_map(|entry| entry.current_version_id)
        .collect();
    let versions = state
        .entry_repo
        .find_versions(&version_ids)
        .await
        .unwrap_or_default();
    let by_id: std::collections::HashMap<_, _> = versions
        .iter()
        .map(|version| (version.id, version))
        .collect();

    recent
        .into_iter()
        .map(|entry| {
            let version = entry.current_version_id.and_then(|id| by_id.get(&id));
            RecentFileDto {
                name: entry
                    .path_norm
                    .as_str()
                    .rsplit('/')
                    .next()
                    .unwrap_or_else(|| entry.path_norm.as_str())
                    .to_string(),
                path: entry.path_norm.as_str().to_string(),
                size_bytes: version
                    .map(|version| version.size_bytes.as_u64())
                    .unwrap_or(0),
                mime_type: version.and_then(|version| version.mime_type.clone()),
                updated_at: version
                    .map(|version| {
                        version
                            .created_at
                            .format(&time::format_description::well_known::Rfc3339)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect()
}
