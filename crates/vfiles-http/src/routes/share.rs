//! Share routes for file sharing functionality.

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    response::{Json, Response},
    routing::{delete, get, post},
};

use crate::{
    AppState,
    dto::{CreateShareRequest, CreateShareResponse, ShareDto},
    error::{ApiError, ApiJson, ApiPath, ApiQuery},
    http_headers::{
        StreamingFileOptions, directory_archive_head_response, streaming_file_response,
        streaming_file_response_with_permit, try_acquire_directory_archive_permit,
    },
    middleware::client_ip_from_headers,
    routes::authenticated_request_context,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use vfiles_domain::{DomainError, EntryKind, NewAuditLog};

const DEFAULT_SHARE_PAGE_LIMIT: u32 = 100;
const MAX_SHARE_PAGE_LIMIT: u32 = 200;

#[derive(Debug, Default, Deserialize)]
struct SharePageQuery {
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Serialize)]
struct SharePageDto {
    items: Vec<ShareDto>,
    total: u64,
    limit: u32,
    offset: u32,
    has_more: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/shares", post(create_share))
        .route("/shares", get(list_shares))
        .route("/shares/page", get(list_shares_page))
        .route("/shares/{code}/download", get(download_share))
        .route("/shares/{code}", get(access_share))
        .route("/shares/{code}", delete(disable_share))
}

fn share_download_audit(
    entry_path: &vfiles_domain::NormalizedPath,
    share_id: &vfiles_domain::ShareId,
) -> NewAuditLog {
    NewAuditLog::success(crate::audit::action::SHARE_DOWNLOAD)
        .target(entry_path.as_str())
        .detail(format!("通过分享链接下载（分享 ID {share_id}）"))
}

fn enforce_share_lookup_rate_limit(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let max_requests_per_minute = state.config.limits.rate_limit_requests_per_minute.max(1);
    // Share codes are bearer credentials. Count guesses across every public lookup
    // route for the source, rather than allowing callers to rotate codes or endpoints.
    let rate_limit_key = format!("share-public:{}", client_ip_from_headers(headers));
    if let Some(block) = state.share_download_limiter.check_and_record(
        max_requests_per_minute,
        std::time::Duration::from_secs(60),
        &rate_limit_key,
    ) {
        tracing::warn!(
            retry_after_secs = block.retry_after_secs,
            "public share lookup rate limit exceeded"
        );
        return Err(ApiError::rate_limited(block.retry_after_secs));
    }

    Ok(())
}

async fn create_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    ApiJson(req): ApiJson<CreateShareRequest>,
) -> Result<Json<CreateShareResponse>, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    let ctx = authenticated_request_context(&state, &jar).await?;
    let user_id = ctx.actor_user_id;

    tracing::info!("Creating share");

    // Parse expiration if provided
    let expires_at = if let Some(expires_str) = req.expires_at {
        Some(
            time::OffsetDateTime::parse(
                &expires_str,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| {
                ApiError::Domain(vfiles_domain::DomainError::Validation {
                    message: "Invalid datetime format, expected RFC3339".to_string(),
                })
            })?,
        )
    } else {
        None
    };

    // Parse path
    let entry_path = vfiles_domain::NormalizedPath::new(&req.path).map_err(|_| {
        ApiError::Domain(vfiles_domain::DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;

    // Create the share
    let code = state
        .share_service
        .create_share(&ctx.namespace_id, &entry_path, expires_at, &user_id)
        .await?;

    crate::audit::record_for(
        &state,
        &headers,
        &ctx,
        NewAuditLog::success(crate::audit::action::SHARE_CREATE)
            .target(entry_path.as_str())
            .detail(match expires_at {
                Some(value) => format!("创建分享链接（有效期至 {value}）"),
                None => "创建分享链接（永久有效）".to_string(),
            }),
    )
    .await;

    let mut share_url = state.config.http.public_base_url.clone();
    share_url.set_path(&format!("/s/{}", code));
    share_url.set_query(None);

    let response = CreateShareResponse {
        code,
        share_url: share_url.to_string(),
    };

    tracing::info!("Share created");

    Ok(Json(response))
}

async fn list_shares(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<ShareDto>>, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    let ctx = authenticated_request_context(&state, &jar).await?;
    let user_id = ctx.actor_user_id;

    tracing::info!("Listing shares for user: {}", user_id.to_string());

    let (shares, total) = state
        .share_service
        .list_shares_with_entry_by_user_page(&user_id, MAX_SHARE_PAGE_LIMIT, 0)
        .await?;
    if total > u64::from(MAX_SHARE_PAGE_LIMIT) {
        return Err(ApiError::Domain(DomainError::Validation {
            message: format!(
                "User has more than {MAX_SHARE_PAGE_LIMIT} active shares; use the paginated endpoint /api/share/shares/page"
            ),
        }));
    }

    let dtos = shares.into_iter().map(ShareDto::from).collect();

    Ok(Json(dtos))
}

async fn list_shares_page(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiQuery(query): ApiQuery<SharePageQuery>,
) -> Result<Json<SharePageDto>, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    let ctx = authenticated_request_context(&state, &jar).await?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_SHARE_PAGE_LIMIT)
        .clamp(1, MAX_SHARE_PAGE_LIMIT);
    let offset = query.offset.unwrap_or(0);
    let (items, total) = state
        .share_service
        .list_shares_with_entry_by_user_page(&ctx.actor_user_id, limit, offset)
        .await?;
    let item_count = u64::try_from(items.len()).unwrap_or(u64::MAX);
    let has_more = u64::from(offset).saturating_add(item_count) < total;

    Ok(Json(SharePageDto {
        items: items.into_iter().map(ShareDto::from).collect(),
        total,
        limit,
        offset,
        has_more,
    }))
}

async fn access_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiPath(code): ApiPath<String>,
) -> Result<Json<ShareDto>, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    enforce_share_lookup_rate_limit(&state, &headers)?;
    tracing::info!("Accessing share");

    let share = state.share_service.access_share(&code).await?;

    let dto = ShareDto::from(share);

    Ok(Json(dto))
}

pub async fn download_share(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    ApiPath(code): ApiPath<String>,
) -> Result<Response, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(DomainError::Forbidden));
    }

    enforce_share_lookup_rate_limit(&state, &headers)?;

    let share = state.share_service.access_share(&code).await?;
    let entry = state.entry_repo.find_by_id(&share.entry_id).await?;

    match entry.entry_type {
        EntryKind::File => {
            let version = share.entry_version_id.map(|value| value.to_string());
            let file = state
                .workspace_service
                .open_file(&share.namespace_id, &entry.path_norm, version.as_deref())
                .await?;

            // The share code is a bearer credential; keep it out of durable audit data.
            crate::audit::record(
                &state,
                &headers,
                share_download_audit(&entry.path_norm, &share.id),
            )
            .await;

            streaming_file_response(
                file.reader,
                StreamingFileOptions {
                    range_allowed: method == Method::GET,
                    request_headers: &headers,
                    mime_type: file.mime_type.as_deref(),
                    size_bytes: file.size_bytes,
                    attachment_filename: Some(&file.filename),
                    etag: Some(&file.etag),
                    modified_at: file.modified_at,
                },
            )
            .await
        }
        EntryKind::Directory => {
            if method == Method::HEAD {
                state
                    .workspace_service
                    .validate_directory_archive_target(&share.namespace_id, &entry.path_norm, None)
                    .await?;
                crate::audit::record(
                    &state,
                    &headers,
                    share_download_audit(&entry.path_norm, &share.id),
                )
                .await;
                let archive_name = entry
                    .path_norm
                    .as_str()
                    .split('/')
                    .rfind(|segment| !segment.is_empty())
                    .unwrap_or("root");
                return directory_archive_head_response(&headers, &format!("{archive_name}.zip"))
                    .await;
            }

            let archive_permit =
                try_acquire_directory_archive_permit().ok_or_else(|| ApiError::rate_limited(1))?;
            let archive = state
                .workspace_service
                .download_directory_archive(&share.namespace_id, &entry.path_norm, None)
                .await?;

            // Audit only after archive generation is admitted and completed.
            crate::audit::record(
                &state,
                &headers,
                share_download_audit(&entry.path_norm, &share.id),
            )
            .await;

            streaming_file_response_with_permit(
                archive.reader,
                StreamingFileOptions {
                    range_allowed: method == Method::GET,
                    request_headers: &headers,
                    mime_type: Some("application/zip"),
                    size_bytes: archive.size_bytes,
                    attachment_filename: Some(&archive.filename),
                    etag: None,
                    modified_at: None,
                },
                archive_permit,
            )
            .await
        }
    }
}

async fn disable_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    ApiPath(code): ApiPath<String>,
) -> Result<StatusCode, ApiError> {
    if !state.config.features.share_enabled {
        return Err(ApiError::Domain(vfiles_domain::DomainError::Forbidden));
    }

    let ctx = authenticated_request_context(&state, &jar).await?;
    let user_id = ctx.actor_user_id;

    tracing::info!(user_id = %user_id, "Disabling share");

    let share_id = state.share_service.disable_share(&code, &user_id).await?;
    tracing::info!(share_id = %share_id, "Share disabled");

    crate::audit::record_for(
        &state,
        &headers,
        &ctx,
        NewAuditLog::success(crate::audit::action::SHARE_DISABLE)
            .target(share_id.to_string())
            .detail("停止分享"),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
