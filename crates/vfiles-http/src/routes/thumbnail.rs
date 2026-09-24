//! Server-side image thumbnails for the file grid.
//!
//! Original photos are far too heavy to download just to render a card, so the
//! grid requests a small JPEG instead. Thumbnails are generated on demand,
//! cached on disk keyed by blob id and requested size, and revalidated with an
//! `ETag` so browsers can keep them in their HTTP cache.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use vfiles_domain::{DomainError, NormalizedPath};

use crate::{
    AppState,
    error::{ApiError, ApiResult},
    http_headers::if_none_match,
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

/// 缩略图磁盘缓存上限与清理目标（按 mtime 回收最旧的条目）。
///
/// 只按条目数设限时，大图缩略图会让缓存体积失控（每张 512px JPEG 可达上百 KB），
/// 因此同时按「条目数 + 总字节数」约束，任一超限都会触发回收。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThumbnailCacheLimits {
    pub max_entries: usize,
    pub target_entries: usize,
    pub max_bytes: u64,
    pub target_bytes: u64,
}

impl ThumbnailCacheLimits {
    /// 由配置构造；清理目标取上限的 80%，与既有的条目数策略保持一致。
    pub fn new(max_entries: usize, max_bytes: u64) -> Self {
        Self {
            max_entries,
            target_entries: max_entries.saturating_mul(4) / 5,
            max_bytes,
            target_bytes: max_bytes / 5 * 4,
        }
    }
}

/// 每写入多少次尝试一次后台清理。
const PRUNE_EVERY_WRITES: u64 = 64;

static WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 缩略图相关的进程内计数。
///
/// 解码失败与「跳过」此前只写日志，排障时无法一眼看出量级；这里累计计数，
/// 并在日志里带上累计值，运维既能看单条也能看趋势（见 `/api/health`）。
#[derive(Debug, Default)]
pub(crate) struct ThumbnailStats {
    /// 命中磁盘缓存
    pub cache_hits: AtomicU64,
    /// 新生成并写盘
    pub generated: AtomicU64,
    /// 因格式不支持或源文件过大而跳过
    pub unsupported: AtomicU64,
    /// 解码/编码失败
    pub failed: AtomicU64,
    /// 缓存回收删除的条目数与字节数
    pub pruned_entries: AtomicU64,
    pub pruned_bytes: AtomicU64,
}

impl ThumbnailStats {
    const fn new() -> Self {
        Self {
            cache_hits: AtomicU64::new(0),
            generated: AtomicU64::new(0),
            unsupported: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            pruned_entries: AtomicU64::new(0),
            pruned_bytes: AtomicU64::new(0),
        }
    }
}

pub(crate) static STATS: ThumbnailStats = ThumbnailStats::new();

/// 供健康检查读取的快照（只读、无锁）。
#[derive(Debug, Serialize)]
pub struct ThumbnailStatsSnapshot {
    pub cache_hits: u64,
    pub generated: u64,
    pub unsupported: u64,
    pub failed: u64,
    pub pruned_entries: u64,
    pub pruned_bytes: u64,
}

impl ThumbnailStats {
    pub(crate) fn snapshot(&self) -> ThumbnailStatsSnapshot {
        ThumbnailStatsSnapshot {
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            generated: self.generated.load(Ordering::Relaxed),
            unsupported: self.unsupported.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            pruned_entries: self.pruned_entries.load(Ordering::Relaxed),
            pruned_bytes: self.pruned_bytes.load(Ordering::Relaxed),
        }
    }
}

/// 健康检查里的缩略图计数。
pub(crate) fn stats_snapshot() -> ThumbnailStatsSnapshot {
    STATS.snapshot()
}

const SUPPORTED_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/tiff",
    "image/x-icon",
    "image/vnd.microsoft.icon",
    "image/qoi",
];

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "ico", "qoi",
];

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

    // Negotiate before reading the source so rejected representations do no storage I/O.
    let format = negotiate_thumbnail_format(
        headers
            .get(header::ACCEPT)
            .and_then(|value| value.to_str().ok()),
        state.config.limits.thumbnail_avif,
    );
    let Some(format) = format else {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NOT_ACCEPTABLE;
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("accept"));
        return Ok(response);
    };

    let file = state
        .workspace_service
        .open_file(&ctx.namespace_id, &path, query.commit.as_deref())
        .await?;

    if !is_supported_image(&file.mime_type, &file.filename) || file.size_bytes > MAX_SOURCE_BYTES {
        let skipped = STATS.unsupported.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::debug!(
            path = %raw_path,
            size_bytes = file.size_bytes,
            skipped_total = skipped,
            "thumbnail skipped: unsupported format or oversized source"
        );
        return Ok(unsupported_response());
    }

    // ETag 与缓存文件都带上格式，避免切换格式命中旧内容。
    let etag = format!(
        "\"{}-{}-{}\"",
        file.etag.trim_matches('"'),
        size,
        format.as_str()
    );
    if if_none_match(&headers, Some(&etag)) {
        return Ok(not_modified_response(&etag));
    }

    let cache_path = thumbnail_cache_path(&state, file.etag.trim_matches('"'), size, format);

    if let Some(bytes) = read_cache(&cache_path).await {
        STATS.cache_hits.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(path = %raw_path, size, format = format.as_str(), "thumbnail cache hit");
        return Ok(thumbnail_response(bytes, &etag, format));
    }

    let mut reader = file.reader.take(file.size_bytes.saturating_add(1));
    let mut source = Vec::with_capacity(file.size_bytes as usize);
    let bytes_read = reader
        .read_to_end(&mut source)
        .await
        .map_err(|err| ApiError::Internal(format!("Failed to read thumbnail source: {err}")))?;
    if bytes_read as u64 != file.size_bytes {
        return Err(ApiError::Internal(
            "Thumbnail source size does not match stored metadata".to_string(),
        ));
    }
    let generated =
        tokio::task::spawn_blocking(move || generate_thumbnail(&source, size, format)).await;

    let bytes = match generated {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(message)) => {
            let failed = STATS.failed.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::warn!(
                path = %raw_path,
                error = %message,
                failed_total = failed,
                "failed to generate thumbnail"
            );
            return Ok(unsupported_response());
        }
        Err(err) => {
            tracing::warn!(path = %raw_path, error = %err, "thumbnail task failed");
            return Err(ApiError::Internal(
                "thumbnail generation failed".to_string(),
            ));
        }
    };

    STATS.generated.fetch_add(1, Ordering::Relaxed);
    tracing::debug!(
        path = %raw_path,
        size,
        format = format.as_str(),
        bytes = bytes.len(),
        "thumbnail generated"
    );

    write_cache(&cache_path, &bytes).await;
    maybe_schedule_prune(
        cache_path.parent().map(PathBuf::from),
        ThumbnailCacheLimits::new(
            state.config.limits.thumbnail_cache_max_entries,
            state.config.limits.thumbnail_cache_max_bytes,
        ),
    );

    Ok(thumbnail_response(bytes, &etag, format))
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

fn thumbnail_cache_path(
    state: &AppState,
    blob_id: &str,
    size: u32,
    format: ThumbnailFormat,
) -> PathBuf {
    state
        .config
        .storage
        .root
        .join("thumbnails")
        .join(format!("{blob_id}-{size}.{}", format.extension()))
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

/// 每隔若干次写入触发一次后台清理，避免目录扫描出现在每个请求上。
fn maybe_schedule_prune(dir: Option<PathBuf>, limits: ThumbnailCacheLimits) {
    let Some(dir) = dir else {
        return;
    };
    let count = WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
    if !count.is_multiple_of(PRUNE_EVERY_WRITES) {
        return;
    }
    tokio::spawn(async move {
        prune_thumbnail_cache(&dir, limits).await;
    });
}

/// 按 mtime 从旧到新删除缩略图，直到条目数与总字节数都不超过清理目标。
///
/// 只统计普通文件；孤儿缩略图（源文件已删除）与失败留下的 `.tmp` 会自然变旧并被回收。
pub(crate) async fn prune_thumbnail_cache(dir: &PathBuf, limits: ThumbnailCacheLimits) {
    let Ok(mut read_dir) = tokio::fs::read_dir(dir).await else {
        return;
    };

    let mut entries: Vec<(PathBuf, std::time::SystemTime, u64)> = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
        entries.push((entry.path(), modified, metadata.len()));
    }

    let mut remaining_entries = entries.len();
    let mut remaining_bytes: u64 = entries.iter().map(|(_, _, size)| *size).sum();
    if remaining_entries <= limits.max_entries && remaining_bytes <= limits.max_bytes {
        return;
    }

    entries.sort_by_key(|(_, modified, _)| *modified);

    let mut removed = 0usize;
    let mut removed_bytes = 0u64;
    let newest = entries.len().saturating_sub(1);
    for (index, (path, _, size)) in entries.iter().enumerate() {
        if remaining_entries <= limits.target_entries && remaining_bytes <= limits.target_bytes {
            break;
        }
        // 始终保留最新的一个条目：单张缩略图超过字节目标时，
        // 若允许清空，缓存会在每个请求上「删光→重新生成」，反而更慢。
        if index == newest {
            break;
        }
        if tokio::fs::remove_file(path).await.is_ok() {
            removed += 1;
            removed_bytes = removed_bytes.saturating_add(*size);
            remaining_entries = remaining_entries.saturating_sub(1);
            remaining_bytes = remaining_bytes.saturating_sub(*size);
        }
    }

    if removed > 0 {
        STATS
            .pruned_entries
            .fetch_add(removed as u64, Ordering::Relaxed);
        STATS
            .pruned_bytes
            .fetch_add(removed_bytes, Ordering::Relaxed);
    }

    tracing::info!(
        removed,
        removed_bytes,
        remaining_entries,
        remaining_bytes,
        "pruned thumbnail cache"
    );
}

/// Decode, downscale and re-encode an image as a small opaque JPEG.
/// 缩略图输出格式：按 `Accept` 协商，支持时优先比 JPEG 更小的 AVIF。
///
/// 这里**不提供 WebP 输出**：`image` crate 只带无损 WebP 编码器，
/// 实测照片类缩略图无损 WebP 反而是 JPEG 的 5 倍多（160KB vs 28KB），
/// 协商到 WebP 会变成性能倒退；需要有损 WebP 时得引入 libwebp（C 依赖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThumbnailFormat {
    Jpeg,
    Avif,
}

impl ThumbnailFormat {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            ThumbnailFormat::Jpeg => "jpg",
            ThumbnailFormat::Avif => "avif",
        }
    }

    pub(crate) fn content_type(self) -> &'static str {
        match self {
            ThumbnailFormat::Jpeg => "image/jpeg",
            ThumbnailFormat::Avif => "image/avif",
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ThumbnailFormat::Jpeg => "jpeg",
            ThumbnailFormat::Avif => "avif",
        }
    }
}

/// 从 `Accept` 头里挑一个可用的图片格式。
///
/// 只认显式的 `image/avif`、`image/jpeg` 与 `image/*`、`*/*` 通配；
/// 按 HTTP quality 和最具体的 media range 选择，所有可用格式都不接受时返回 `None`。
pub(crate) fn negotiate_thumbnail_format(
    accept: Option<&str>,
    allow_avif: bool,
) -> Option<ThumbnailFormat> {
    let Some(accept) = accept else {
        return Some(ThumbnailFormat::Jpeg);
    };

    // Keep malformed q values at zero: treating them as the default q=1 can
    // send a representation the client explicitly tried to exclude.
    let mut entries: Vec<(String, f32)> = Vec::new();
    for part in accept.split(',') {
        let mut segments = part.split(';');
        let Some(media) = segments.next() else {
            continue;
        };
        let media = media.trim().to_ascii_lowercase();
        if media.is_empty() {
            continue;
        }

        let mut quality = 1.0_f32;
        for parameter in segments {
            let parameter = parameter.trim();
            if let Some((name, value)) = parameter.split_once('=')
                && name.trim().eq_ignore_ascii_case("q")
            {
                quality = parse_quality(value.trim()).unwrap_or(0.0);
            }
        }
        entries.push((media, quality));
    }

    let quality_for = |candidate: &str| -> f32 {
        let specificity = |media: &str| match media {
            value if value == candidate => 2,
            "image/*" => 1,
            "*/*" => 0,
            _ => -1,
        };
        let best_specificity = entries
            .iter()
            .map(|(media, _)| specificity(media))
            .max()
            .unwrap_or(-1);
        if best_specificity < 0 {
            return 0.0;
        }
        entries
            .iter()
            .filter(|(media, _)| specificity(media) == best_specificity)
            .map(|(_, quality)| *quality)
            .fold(0.0_f32, f32::max)
    };

    // AVIF 编码开销远高于 JPEG（实测 384px 约 2.0s vs 0.004s），默认关闭，
    // 由 `VFILES_THUMBNAIL_AVIF` 决定是否参与协商
    let mut supported = vec![("image/jpeg", ThumbnailFormat::Jpeg)];
    if allow_avif {
        supported.insert(0, ("image/avif", ThumbnailFormat::Avif));
    }

    let mut selected = None;
    for (candidate, format) in supported {
        let quality = quality_for(candidate);
        if quality > 0.0 && selected.is_none_or(|(_, best_quality)| quality > best_quality) {
            // Keep AVIF on equal quality by listing it before JPEG.
            selected = Some((format, quality));
        }
    }
    selected.map(|(format, _)| format)
}

fn parse_quality(value: &str) -> Option<f32> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if !matches!(whole, "0" | "1")
        || (value.contains('.') && fraction.is_empty())
        || fraction.len() > 3
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    if whole == "1" && fraction.bytes().any(|digit| digit != b'0') {
        return None;
    }
    value.parse().ok()
}

/// 按目标格式编码缩略图，输入是已经扁平化到白色背景的 RGB 图。
fn encode_thumbnail(format: ThumbnailFormat, image: &image::RgbImage) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    match format {
        ThumbnailFormat::Jpeg => {
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, JPEG_QUALITY);
            encoder.encode_image(image).map_err(|err| err.to_string())?;
        }
        ThumbnailFormat::Avif => {
            // speed 越大越快、压缩率越低。实测 384px 缩略图 speed=6 需 5.3s、
            // speed=10 约 0.4s，体积仍远小于 JPEG，因此取最快档。
            use image::ImageEncoder;
            let encoder =
                image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut output, 10, 60);
            encoder
                .write_image(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|err| err.to_string())?;
        }
    }
    Ok(output)
}

fn generate_thumbnail(
    source: &[u8],
    size: u32,
    format: ThumbnailFormat,
) -> Result<Vec<u8>, String> {
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

    encode_thumbnail(format, &flattened)
}

fn thumbnail_response(bytes: Vec<u8>, etag: &str, format: ThumbnailFormat) -> Response {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(format.content_type()),
    );
    // 同一 URL 会因 Accept 返回不同格式，必须声明 Vary，避免中间缓存串味
    headers.insert(header::VARY, HeaderValue::from_static("accept"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn set_modified(path: &PathBuf, modified: SystemTime) {
        let file = std::fs::File::options()
            .write(true)
            .open(path)
            .expect("cached file should open");
        file.set_modified(modified).expect("mtime should be set");
    }

    fn cached_names(dir: &PathBuf) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("cache dir should list")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// 写入 `count` 个固定大小的条目，mtime 依次递增（越大越新）。
    async fn seed_cache(dir: &std::path::Path, count: usize, bytes: usize) -> Vec<PathBuf> {
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let mut paths = Vec::new();
        for index in 0..count {
            let path = dir.join(format!("thumb-{index}.jpg"));
            tokio::fs::write(&path, vec![b'x'; bytes])
                .await
                .expect("write");
            set_modified(&path, base + Duration::from_secs(index as u64 * 60));
            paths.push(path);
        }
        paths
    }

    #[tokio::test]
    async fn prune_removes_oldest_entries_down_to_target() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let dir_path = dir.path().to_path_buf();
        seed_cache(&dir_path, 5, 5).await;

        // 条目数上限 3、目标 2；字节上限足够大，不参与决策
        prune_thumbnail_cache(&dir_path, ThumbnailCacheLimits::new(3, u64::MAX)).await;

        assert_eq!(cached_names(&dir_path), vec!["thumb-3.jpg", "thumb-4.jpg"]);
    }

    #[tokio::test]
    async fn prune_is_a_noop_below_the_limit() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let dir_path = dir.path().to_path_buf();
        tokio::fs::write(dir_path.join("only.jpg"), b"thumb")
            .await
            .expect("write");

        prune_thumbnail_cache(&dir_path, ThumbnailCacheLimits::new(3, 1024)).await;

        assert_eq!(cached_names(&dir_path), vec!["only.jpg"]);
    }

    #[tokio::test]
    async fn prune_enforces_the_byte_limit_even_below_the_entry_limit() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let dir_path = dir.path().to_path_buf();
        // 5 条 × 100 字节 = 500 字节；字节上限 250、目标 200
        seed_cache(&dir_path, 5, 100).await;
        // 计数是进程级的，测试并发执行时只会更大，因此用「增量下界」断言
        let before = STATS.snapshot();

        prune_thumbnail_cache(&dir_path, ThumbnailCacheLimits::new(100, 250)).await;

        let after = STATS.snapshot();
        assert!(
            after.pruned_entries >= before.pruned_entries + 3,
            "pruned entries counter should grow by at least the three files removed"
        );
        assert!(
            after.pruned_bytes >= before.pruned_bytes + 300,
            "pruned bytes counter should grow by at least the bytes removed"
        );

        let names = cached_names(&dir_path);
        assert_eq!(
            names,
            vec!["thumb-3.jpg", "thumb-4.jpg"],
            "应保留最新的 2 条（200 字节）以满足字节目标"
        );
        let total: u64 = names
            .iter()
            .map(|name| std::fs::metadata(dir_path.join(name)).expect("meta").len())
            .sum();
        assert_eq!(total, 200);
    }

    #[tokio::test]
    async fn prune_keeps_a_single_oversized_entry() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let dir_path = dir.path().to_path_buf();
        // 单条 500 字节已超过 250 字节上限：仍会删到只剩它（不能删空）
        seed_cache(&dir_path, 3, 250).await;

        prune_thumbnail_cache(&dir_path, ThumbnailCacheLimits::new(100, 250)).await;

        assert_eq!(cached_names(&dir_path), vec!["thumb-2.jpg"]);
    }

    /// 用 image 编码器生成一张纯色图，覆盖新增格式的解码路径。
    fn encode_sample(format: image::ImageFormat) -> Vec<u8> {
        use image::{Rgba, RgbaImage};
        use std::io::Cursor;

        // ICO 解码要求内嵌 PNG 为 RGBA，因此统一用带 alpha 的样本
        let mut image = RgbaImage::new(8, 8);
        for pixel in image.pixels_mut() {
            *pixel = Rgba([10, 120, 200, 255]);
        }

        let mut buffer = Cursor::new(Vec::new());
        image
            .write_to(&mut buffer, format)
            .expect("sample image should encode");
        buffer.into_inner()
    }

    #[test]
    fn supports_tiff_ico_and_qoi_sources() {
        for (format, extension, mime) in [
            (image::ImageFormat::Tiff, "tiff", "image/tiff"),
            (image::ImageFormat::Ico, "ico", "image/x-icon"),
            (image::ImageFormat::Qoi, "qoi", "image/qoi"),
            (image::ImageFormat::Png, "png", "image/png"),
        ] {
            let source = encode_sample(format);
            let thumbnail = generate_thumbnail(&source, 64, ThumbnailFormat::Jpeg)
                .unwrap_or_else(|err| panic!("{extension} should decode: {err}"));
            // 结果统一是 JPEG
            assert_eq!(&thumbnail[..3], &[0xFF, 0xD8, 0xFF], "{extension} output");

            assert!(
                is_supported_image(&None, &format!("photo.{extension}")),
                "{extension} should be accepted by extension"
            );
            assert!(
                is_supported_image(&Some(mime.to_string()), "photo"),
                "{extension} should be accepted by mime {mime}"
            );
        }
    }

    #[test]
    fn rejects_formats_without_a_decoder() {
        assert!(!is_supported_image(&Some("image/svg+xml".to_string()), "x"));
        assert!(!is_supported_image(&None, "scan.pdf"));
        assert!(!is_supported_image(
            &Some("image/avif".to_string()),
            "photo.avif"
        ));
    }

    #[test]
    fn negotiates_output_format_from_accept() {
        use super::{ThumbnailFormat, negotiate_thumbnail_format};

        let avif = |accept: Option<&str>| negotiate_thumbnail_format(accept, true);
        let no_avif = |accept: Option<&str>| negotiate_thumbnail_format(accept, false);

        // 开启 AVIF 时，浏览器典型 Accept 优先 AVIF
        assert_eq!(
            avif(Some("image/avif,image/webp,image/apng,image/*,*/*;q=0.8")),
            Some(ThumbnailFormat::Avif)
        );
        // 未开启 AVIF 时即使客户端支持也回退 JPEG
        assert_eq!(
            no_avif(Some("image/avif,image/webp,image/apng,image/*,*/*;q=0.8")),
            Some(ThumbnailFormat::Jpeg)
        );
        // 只声明未支持格式时，不返回未被接受的 JPEG 或 AVIF。
        assert_eq!(avif(Some("image/webp,image/png")), None);
        // 声明了 WebP 但同时也接受 */* 时，可以给更小的 AVIF
        assert_eq!(
            avif(Some("image/webp,*/*;q=0.5")),
            Some(ThumbnailFormat::Avif)
        );
        // 老客户端只接受 JPEG
        assert_eq!(
            avif(Some("image/jpeg,image/png")),
            Some(ThumbnailFormat::Jpeg)
        );
        // 显式列出 PNG（不在支持列表）+ 通配时，显式声明优先于通配
        assert_eq!(
            avif(Some("image/png,*/*;q=0.5")),
            Some(ThumbnailFormat::Avif)
        );
        // 明确拒绝 AVIF（q=0）时退回 JPEG
        assert_eq!(
            avif(Some("image/avif;q=0,image/jpeg")),
            Some(ThumbnailFormat::Jpeg)
        );
        assert_eq!(avif(Some("*/*")), Some(ThumbnailFormat::Avif));
        assert_eq!(avif(None), Some(ThumbnailFormat::Jpeg));
        assert_eq!(avif(Some("text/html")), None);
        assert_eq!(avif(Some("image/avif;q=0,image/jpeg;q=0")), None);
        assert_eq!(
            avif(Some("image/avif;q=0.1,image/jpeg;q=1")),
            Some(ThumbnailFormat::Jpeg)
        );
        assert_eq!(
            avif(Some("image/avif;q=bogus,*/*;q=0.8")),
            Some(ThumbnailFormat::Jpeg)
        );
        assert_eq!(
            avif(Some("image/avif;Q=0.7,image/jpeg;q=0.6")),
            Some(ThumbnailFormat::Avif)
        );
        assert_eq!(
            avif(Some("image/avif;q=0.,image/jpeg;q=0.5")),
            Some(ThumbnailFormat::Jpeg)
        );
    }

    #[test]
    fn encodes_every_supported_output_format() {
        use super::{ThumbnailFormat, encode_thumbnail, generate_thumbnail};

        let source = encode_sample(image::ImageFormat::Png);
        let rgb = image::RgbImage::new(4, 4);

        // 两种格式都能编码出非空数据，且走完整链路时产出对应魔数
        assert!(encode_thumbnail(ThumbnailFormat::Jpeg, &rgb).is_ok());
        let avif = encode_thumbnail(ThumbnailFormat::Avif, &rgb).expect("avif");
        assert_eq!(&avif[4..8], b"ftyp");

        let jpeg = generate_thumbnail(&source, 4, ThumbnailFormat::Jpeg).expect("jpeg chain");
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8]);
        let avif_chain = generate_thumbnail(&source, 4, ThumbnailFormat::Avif).expect("avif chain");
        assert_eq!(&avif_chain[4..8], b"ftyp");
    }

    #[test]
    fn cache_limits_derive_targets_from_the_maximum() {
        let limits = ThumbnailCacheLimits::new(2000, 256 * 1024 * 1024);

        assert_eq!(limits.max_entries, 2000);
        assert_eq!(limits.target_entries, 1600);
        assert_eq!(limits.max_bytes, 268_435_456);
        assert_eq!(limits.target_bytes, 214_748_364);
    }
}
