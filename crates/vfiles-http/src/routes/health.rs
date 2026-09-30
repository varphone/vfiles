//! Health check endpoints.

use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use time::OffsetDateTime;

use crate::{
    AppState,
    error::ApiResult,
    routes::thumbnail::{ThumbnailStatsSnapshot, stats_snapshot},
};
use vfiles_app::IngestStatsSnapshot;
use vfiles_infra_sqlite::SqliteHealthProbe;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
    /// 缩略图计数：解码失败、格式跳过、缓存命中与回收量，便于排障与容量评估。
    pub thumbnail: ThumbnailStatsSnapshot,
    /// FTP 批量导入计数：会话、登录、上传/下载字节与快照提交量。
    pub ftp: IngestStatsSnapshot,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub checks: Vec<HealthCheck>,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
}

pub async fn health_check(State(state): State<AppState>) -> ApiResult<Json<HealthResponse>> {
    tracing::debug!("Health check requested");
    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        thumbnail: stats_snapshot(),
        ftp: state.ingest_stats.snapshot(),
    }))
}

pub async fn readiness_check(
    State(state): State<AppState>,
) -> ApiResult<(StatusCode, Json<ReadinessResponse>)> {
    tracing::debug!("Readiness check requested");
    let mut checks = Vec::new();

    // Check database
    tracing::debug!("Checking database readiness...");
    match SqliteHealthProbe::check_readiness(&state.db_pool).await {
        Ok(_) => {
            tracing::debug!("Database check passed");
            checks.push(HealthCheck {
                name: "database".to_string(),
                status: "ok".to_string(),
                message: None,
            });
        }
        Err(e) => {
            tracing::error!("Database check failed: {}", e);
            checks.push(HealthCheck {
                name: "database".to_string(),
                status: "error".to_string(),
                message: Some("Database is unavailable".to_string()),
            });
        }
    }

    // Check that the writable storage directories used by uploads and blobs are
    // available, not only that the root directory exists.
    tracing::debug!("Checking storage readiness...");
    match vfiles_infra_fs::FsHealthProbe::check_readiness(&state.config.storage.root).await {
        Ok(()) => {
            tracing::debug!("Storage check passed");
            checks.push(HealthCheck {
                name: "storage".to_string(),
                status: "ok".to_string(),
                message: None,
            });
        }
        Err(error) => {
            tracing::error!("Storage check failed: {}", error);
            checks.push(HealthCheck {
                name: "storage".to_string(),
                status: "error".to_string(),
                message: Some("Storage is unavailable or not writable".to_string()),
            });
        }
    }

    let ready = checks.iter().all(|check| check.status == "ok");
    let overall_status = if ready { "ready" } else { "not_ready" };
    let http_status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    Ok((
        http_status,
        Json(ReadinessResponse {
            status: overall_status.to_string(),
            checks,
            timestamp: OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        }),
    ))
}
