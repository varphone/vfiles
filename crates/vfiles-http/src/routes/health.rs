//! Health check endpoints.

use axum::{Json, extract::State};
use serde::Serialize;
use time::OffsetDateTime;

use crate::{AppState, error::ApiResult};

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
    match state.health_service.check_readiness().await {
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
