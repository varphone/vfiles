//! Health check endpoints.

use axum::{Json, extract::State};
use serde::Serialize;
use time::OffsetDateTime;

use crate::{AppState, error::ApiResult};
use vfiles_infra_sqlite::SqliteHealthProbe;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
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

pub async fn health_check() -> Json<HealthResponse> {
    tracing::debug!("Health check requested");
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap(),
    })
}

pub async fn readiness_check(State(state): State<AppState>) -> ApiResult<Json<ReadinessResponse>> {
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
                message: Some(format!("Database check failed: {}", e)),
            });
        }
    }

    // Check that the storage root still exists and is a directory. A missing
    // storage root would make uploads/downloads fail even though the API process
    // itself is alive.
    tracing::debug!("Checking storage readiness...");
    let storage_root = state.config.storage.root.as_std_path();
    match tokio::fs::metadata(storage_root).await {
        Ok(metadata) if metadata.is_dir() => {
            tracing::debug!("Storage check passed");
            checks.push(HealthCheck {
                name: "storage".to_string(),
                status: "ok".to_string(),
                message: None,
            });
        }
        Ok(_) => {
            tracing::error!(
                "Storage root is not a directory: {}",
                storage_root.display()
            );
            checks.push(HealthCheck {
                name: "storage".to_string(),
                status: "error".to_string(),
                message: Some("Storage root is not a directory".to_string()),
            });
        }
        Err(e) => {
            tracing::error!("Storage check failed: {}", e);
            checks.push(HealthCheck {
                name: "storage".to_string(),
                status: "error".to_string(),
                message: Some(format!("Storage root is unavailable: {}", e)),
            });
        }
    }

    let overall_status = if checks.iter().all(|c| c.status == "ok") {
        "ready"
    } else {
        "not_ready"
    };

    Ok(Json(ReadinessResponse {
        status: overall_status.to_string(),
        checks,
        timestamp: OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap(),
    }))
}
