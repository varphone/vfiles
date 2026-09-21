//! 工作区聚合统计：侧栏「存储用量 / 最近文件」的数据来源。

use axum::{extract::State, response::Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;

use crate::{AppState, error::ApiResult, routes::protected_request_context};

/// 「最近」返回的文件数量。
const RECENT_FILE_LIMIT: u32 = 8;

#[derive(Debug, Serialize)]
pub struct RecentFileDto {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
    pub updated_at: String,
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
    axum::Router::new().route("/overview", axum::routing::get(overview))
}

pub async fn overview(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<Json<OverviewDto>> {
    let ctx = protected_request_context(&state, &jar).await?;

    // 统计走 SQL 聚合，避免把整棵目录树拉进内存
    let stats = state.entry_repo.stats(&ctx.namespace_id).await?;
    // 分类占比是辅助信息：失败不影响总量与最近文件
    let categories = state
        .entry_repo
        .stats_by_category(&ctx.namespace_id)
        .await
        .unwrap_or_default();
    let recent = state
        .entry_repo
        .recent_files(&ctx.namespace_id, RECENT_FILE_LIMIT)
        .await?;

    // 最近文件需要展示大小与类型：按 entry 的当前版本补齐
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

    let recent_files = recent
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
        .collect();

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
