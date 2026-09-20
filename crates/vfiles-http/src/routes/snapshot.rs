//! Snapshot management routes.

use axum::{Json, Router, routing::post};
use axum_extra::extract::cookie::CookieJar;

use crate::{
    AppState,
    dto::{CreateSnapshotRequest, SnapshotDto},
    error::{ApiJson, ApiResult},
    routes::protected_request_context,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/snapshots", post(create_snapshot))
}

pub async fn create_snapshot(
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    ApiJson(req): ApiJson<CreateSnapshotRequest>,
) -> ApiResult<Json<SnapshotDto>> {
    let ctx = protected_request_context(&state, &jar).await?;
    let snapshot = state
        .workspace_service
        .create_snapshot(
            &ctx.namespace_id,
            req.message.as_deref(),
            &ctx.actor_user_id,
        )
        .await?;
    let dto: SnapshotDto = snapshot.into();
    Ok(Json(dto))
}
