//! Server-side image thumbnails for the file grid.
//!
//! Original photos are far too heavy to download just to render a card, so the
//! grid requests a small JPEG instead. Thumbnails are generated on demand,
//! cached on disk keyed by blob id and requested size, and revalidated with an
//! `ETag` so browsers can keep them in their HTTP cache.

use std::path::PathBuf;

use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use vfiles_domain::{DomainError, NormalizedPath};

use crate::{
    AppState,
    error::{ApiError, ApiResult},
    routes::protected_request_context,
};

const MIN_SIZE: u32 = 64;
const MAX_SIZE: u32 = 512;
const DEFAULT_SIZE: u32 = 256;
/// Skip absurdly large sources instead of buffering/decoding them.
const MAX_SOURCE_BYTES: u64 = 40 * 1024 * 1024;
/// Guard against decompression bombs from small files with huge dimensions.
const MAX_SOURCE_PIXELS: u64 = 50_000_000;
const MAX_SOURCE_DIMENSION: u32 = 16_384;
const JPEG_QUALITY: u8 = 82;
const CACHE_CONTROL: &str = "private, max-age=604800";

const SUPPORTED_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
];

const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

#[derive(Debug, Deserialize)]
struct ThumbnailQuery {
    path: Option<String>,
    commit: Option<String>,
    size: Option<u32>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/thumbnail", get(get_file_thumbnail))
}

async fn get_file_thumbnail(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(query): Query<ThumbnailQuery>,
) -> ApiResult<Response> {
    let ctx = protected_request_context(&state, &jar).await?;
    let raw_path = query.path.ok_or_else(|| {
        ApiError::Domain(DomainError::Validation {
            message: "Missing path parameter".to_string(),
        })
    })?;
    let path = NormalizedPath::new(&raw_path).map_err(|_| {
        ApiError::Domain(DomainError::Validation {
            message: "Invalid path format".to_string(),
        })
    })?;
    let size = query.size.unwrap_or(DEFAULT_SIZE).clamp(MIN_SIZE, MAX_SIZE);

    let file = state
        .workspace_service
        .read_file_bytes(&ctx.namespace_id, &path, query.commit.as_deref())
        .await?;

    if !is_supported_image(&file.mime_type, &file.filename) || file.size_bytes > MAX_SOURCE_BYTES {
        return Ok(unsupported_response());
    }

    let etag = format!("\"{}-{}\"", file.blob_id, size);
    if is_not_modified(&headers, &etag) {
        return Ok(not_modified_response(&etag));
    }

    let cache_path = thumbnail_cache_path(&state, &file.blob_id.to_string(), size);

    if let Some(bytes) = read_cache(&cache_path).await {
        return Ok(thumbnail_response(bytes, &etag));
    }

    let source = file.bytes;
    let generated = tokio::task::spawn_blocking(move || generate_thumbnail(&source, size)).await;

    let bytes = match generated {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(message)) => {
            tracing::warn!(path = %raw_path, error = %message, "failed to generate thumbnail");
            return Ok(unsupported_response());
        }
        Err(err) => {
            tracing::warn!(path = %raw_path, error = %err, "thumbnail task failed");
            return Err(ApiError::Internal(
                "thumbnail generation failed".to_string(),
            ));
        }
    };

    write_cache(&cache_path, &bytes).await;

    Ok(thumbnail_response(bytes, &etag))
}

fn is_supported_image(mime_type: &Option<String>, filename: &str) -> bool {
    if let Some(mime_type) = mime_type {
        return SUPPORTED_MIME_TYPES
            .iter()
            .any(|candidate| mime_type.eq_ignore_ascii_case(candidate));
    }

    let extension = filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|candidate| extension == *candidate)
}

fn thumbnail_cache_path(state: &AppState, blob_id: &str, size: u32) -> PathBuf {
    state
        .config
        .storage
        .root
        .join("thumbnails")
        .join(format!("{blob_id}-{size}.jpg"))
        .into_std_path_buf()
}

async fn read_cache(path: &PathBuf) -> Option<Vec<u8>> {
    match tokio::fs::read(path).await {
        Ok(bytes) if !bytes.is_empty() => Some(bytes),
        _ => None,
    }
}

async fn write_cache(path: &PathBuf, bytes: &[u8]) {
    let Some(parent) = path.parent() else {
        return;
    };
    if tokio::fs::create_dir_all(parent).await.is_err() {
        return;
    }

    let temp_path = path.with_extension("jpg.tmp");
    if tokio::fs::write(&temp_path, bytes).await.is_err() {
        return;
    }
    // Rename is atomic on the same filesystem, so concurrent readers never see
    // a partially written thumbnail.
    if let Err(err) = tokio::fs::rename(&temp_path, path).await {
        tracing::debug!(error = %err, "failed to persist thumbnail cache entry");
        let _ = tokio::fs::remove_file(&temp_path).await;
    }
}

/// Decode, downscale and re-encode an image as a small opaque JPEG.
fn generate_thumbnail(source: &[u8], size: u32) -> Result<Vec<u8>, String> {
    use image::{ImageReader, Limits, Rgb, RgbImage};

    let mut reader = ImageReader::new(std::io::Cursor::new(source))
        .with_guessed_format()
        .map_err(|err| err.to_string())?;

    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_SOURCE_DIMENSION);
    limits.max_alloc = Some(MAX_SOURCE_PIXELS.saturating_mul(4));
    reader.limits(limits);

    let image = reader.decode().map_err(|err| err.to_string())?;
    let thumbnail = image.thumbnail(size, size);

    // Flatten transparency over white so JPEG encoding stays lossless-looking.
    let rgba = thumbnail.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut flattened = RgbImage::new(width, height);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = u32::from(pixel[3]);
        let blend = |channel: u8| -> u8 {
            let value = u32::from(channel) * alpha + 255 * (255 - alpha);
            (value / 255) as u8
        };
        flattened.put_pixel(
            x,
            y,
            Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])]),
        );
    }

    let mut output = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, JPEG_QUALITY);
    encoder
        .encode_image(&flattened)
        .map_err(|err| err.to_string())?;
    Ok(output)
}

fn is_not_modified(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == etag || value.trim() == "*")
}

fn thumbnail_response(bytes: Vec<u8>, etag: &str) -> Response {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL),
    );
    if let Ok(value) = HeaderValue::from_str(etag) {
        headers.insert(header::ETAG, value);
    }
    response
}

fn not_modified_response(etag: &str) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_MODIFIED;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL),
    );
    if let Ok(value) = HeaderValue::from_str(etag) {
        headers.insert(header::ETAG, value);
    }
    response
}

fn unsupported_response() -> Response {
    (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported image type").into_response()
}
