//! File history routes.

use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult},
    routes::protected_request_context,
};
use vfiles_app::{DirectoryHistoryPage, EntryHistoryPage};
use vfiles_domain::{ChangeType, DomainError, SnapshotKind, VersionId};

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    path: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiffQuery {
    path: Option<String>,
    commit: Option<String>,
    parent: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RestoreVersionRequest {
    path: String,
    commit: String,
    message: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_entry_history))
        .route("/restore", post(restore_entry_version))
        .route("/diff", get(get_entry_diff))
}

fn parse_version_id(raw: Option<&str>, field: &str) -> ApiResult<Option<VersionId>> {
    raw.filter(|value| !value.trim().is_empty())
        .map(|value| {
            VersionId::from_string(value).map_err(|_| {
                ApiError::Domain(DomainError::Validation {
                    message: format!("Invalid {} parameter", field),
                })
            })
        })
        .transpose()
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}

fn change_type_label(change_type: ChangeType) -> &'static str {
    match change_type {
        ChangeType::Added => "added",
        ChangeType::Modified => "modified",
        ChangeType::Deleted => "deleted",
        ChangeType::Renamed => "renamed",
    }
}

fn file_history_payload(page: EntryHistoryPage) -> serde_json::Value {
    let default_message = format!("更新 {}", basename(&page.path));
    let current_version = page
        .current_version_id
        .map(|value| value.to_string())
        .or_else(|| page.items.first().map(|item| item.version_id.to_string()))
        .unwrap_or_default();

    serde_json::json!({
        "commits": page.items.iter().enumerate().map(|(index, item)| {
            serde_json::json!({
                "hash": item.version_id.to_string(),
                "message": item.message.clone().unwrap_or_else(|| default_message.clone()),
                "changeType": change_type_label(item.change_type),
                "hasCustomMessage": item.has_custom_message,
                "author": {
                    "name": item.actor_name,
                    "email": "",
                },
                "date": format_timestamp(item.created_at),
                "parent": page.items.get(index + 1).map(|next| vec![next.version_id.to_string()]).unwrap_or_default(),
            })
        }).collect::<Vec<_>>(),
        "currentVersion": current_version,
        "totalCommits": page.total_items,
        "nextCursor": page.next_cursor,
    })
}

fn directory_history_payload(page: DirectoryHistoryPage) -> serde_json::Value {
    let current_version = page
        .current_snapshot_id
        .map(|value| value.to_string())
        .unwrap_or_default();

    serde_json::json!({
        "commits": page.items.iter().enumerate().map(|(index, item)| {
            serde_json::json!({
                "hash": item.snapshot_id.to_string(),
                "message": item.message.clone().unwrap_or_else(|| match item.kind {
                    SnapshotKind::UserCreated => "创建快照".to_string(),
                    SnapshotKind::AutoCommit => "自动快照".to_string(),
                }),
                "author": {
                    "name": item.actor_name,
                    "email": "",
                },
                "date": format_timestamp(item.created_at),
                "parent": page.items.get(index + 1).map(|next| vec![next.snapshot_id.to_string()]).unwrap_or_default(),
            })
        }).collect::<Vec<_>>(),
        "currentVersion": current_version,
        "totalCommits": page.total_items,
    })
}

async fn get_entry_history(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<impl IntoResponse> {
    let ctx = protected_request_context(&state, &jar).await?;
    if !state.config.features.history_enabled {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }

    let raw_path = query.path.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "Missing path parameter".to_string(),
        })
    })?;
    let normalized_path = vfiles_domain::NormalizedPath::new(&raw_path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let history = match state
        .history_service
        .entry_history(
            &ctx.namespace_id,
            &normalized_path,
            query.cursor.as_deref(),
            limit,
        )
        .await
    {
        Ok(page) => file_history_payload(page),
        Err(DomainError::Validation { .. }) | Err(DomainError::NotFound { .. }) => {
            let page = state
                .history_service
                .directory_history(&ctx.namespace_id, &normalized_path, limit)
                .await?;
            directory_history_payload(page)
        }
        Err(err) => return Err(err.into()),
    };

    Ok(Json(serde_json::json!({
        "success": true,
        "data": history,
    })))
}

async fn get_entry_diff(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<DiffQuery>,
) -> ApiResult<impl IntoResponse> {
    let ctx = protected_request_context(&state, &jar).await?;
    if !state.config.features.history_enabled {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }

    let raw_path = query.path.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "Missing path parameter".to_string(),
        })
    })?;
    let normalized_path = vfiles_domain::NormalizedPath::new(&raw_path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;
    let commit = parse_version_id(query.commit.as_deref(), "commit")?.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "Missing commit parameter".to_string(),
        })
    })?;
    let parent = parse_version_id(query.parent.as_deref(), "parent")?;

    let diff = state
        .history_service
        .diff_entry(
            &ctx.namespace_id,
            &normalized_path,
            &commit,
            parent.as_ref(),
        )
        .await?;

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        diff,
    ))
}

async fn restore_entry_version(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiJson(req): ApiJson<RestoreVersionRequest>,
) -> ApiResult<impl IntoResponse> {
    let ctx = protected_request_context(&state, &jar).await?;
    if !state.config.features.history_enabled {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }

    let normalized_path = vfiles_domain::NormalizedPath::new(&req.path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;
    let version_id = VersionId::from_string(&req.commit).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid commit parameter".to_string(),
        })
    })?;

    let message = req.message.as_deref();
    let mutation = state
        .history_service
        .restore_version(
            &ctx.namespace_id,
            &normalized_path,
            &version_id,
            message,
            &ctx.actor_user_id,
        )
        .await?;

    let restored_version_id = mutation
        .changed_entries
        .iter()
        .find(|entry| entry.path == normalized_path.as_str())
        .and_then(|entry| entry.current_version_id)
        .map(|value| value.to_string());

    Ok(Json(serde_json::json!({
        "success": true,
        "data": {
            "path": normalized_path.as_str(),
            "restoredFrom": version_id.to_string(),
            "currentVersion": restored_version_id,
            "snapshotId": mutation.snapshot_id.to_string(),
        }
    })))
}
