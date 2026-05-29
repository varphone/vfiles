//! Session management routes.

use axum::{Json, Router, routing::get};

use crate::{AppState, dto::SessionBootstrapDto, error::ApiResult, routes::request_context};

pub fn router() -> Router<AppState> {
    Router::new().route("/bootstrap", get(bootstrap_session))
}

pub async fn bootstrap_session(
    axum::extract::State(state): axum::extract::State<AppState>,
    cookies: axum_extra::extract::CookieJar,
) -> ApiResult<Json<SessionBootstrapDto>> {
    let auth_token = cookies.get("auth_token").map(|c| c.value());
    let ctx = request_context(&state, &cookies).await?;
    let mut bootstrap = state.session_service.bootstrap(auth_token).await?;
    bootstrap.active_workspace = Some(ctx.namespace_id);
    Ok(Json(SessionBootstrapDto::from(bootstrap)))
}
