use axum::{
    body::Body, http::StatusCode, http::header, response::IntoResponse, response::Response,
};
use std::path::{Component, Path, PathBuf};

#[cfg(feature = "embed")]
use include_dir::{Dir, include_dir};

#[cfg(feature = "embed")]
static EMBEDDED_FRONTEND: Dir<'static> = include_dir!("$VFILES_EMBED_FRONTEND_DIST");

#[derive(Clone, Debug)]
pub enum FrontendAssets {
    Filesystem(PathBuf),
    #[cfg(feature = "embed")]
    Embedded,
}

#[derive(Debug)]
struct FrontendRequestPath {
    segments: Vec<String>,
    is_resource_like: bool,
}

/// 构建期预压缩的编码变体，读取 `<asset>.br` / `<asset>.gz`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Precompressed {
    Brotli,
    Gzip,
}

impl Precompressed {
    fn suffix(self) -> &'static str {
        match self {
            Self::Brotli => ".br",
            Self::Gzip => ".gz",
        }
    }

    fn header_value(self) -> &'static str {
        match self {
            Self::Brotli => "br",
            Self::Gzip => "gzip",
        }
    }
}

impl FrontendAssets {
    pub fn filesystem(path: PathBuf) -> Option<Self> {
        (path.is_dir() && path.join("index.html").is_file()).then_some(Self::Filesystem(path))
    }

    #[cfg(feature = "embed")]
    pub fn embedded() -> Option<Self> {
        EMBEDDED_FRONTEND
            .get_file("index.html")
            .map(|_| Self::Embedded)
    }

    pub fn is_available(&self) -> bool {
        match self {
            Self::Filesystem(path) => path.is_dir() && path.join("index.html").is_file(),
            #[cfg(feature = "embed")]
            Self::Embedded => EMBEDDED_FRONTEND.get_file("index.html").is_some(),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Filesystem(path) => format!("filesystem: {}", path.display()),
            #[cfg(feature = "embed")]
            Self::Embedded => "embedded frontend assets".to_string(),
        }
    }

    /// 提供静态资源；`accept_encoding` 用于选择构建期预压缩的变体。
    pub async fn serve(&self, uri_path: &str, accept_encoding: Option<&str>) -> Response {
        let Some(request_path) = resolve_request_path(uri_path) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let encoding = preferred_precompression(accept_encoding);

        if let Some(response) = self.serve_requested_path(&request_path, encoding).await {
            return response;
        }

        if request_path.is_resource_like {
            return StatusCode::NOT_FOUND.into_response();
        }

        self.serve_index(encoding)
            .await
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
    }

    async fn serve_requested_path(
        &self,
        request_path: &FrontendRequestPath,
        encoding: Option<Precompressed>,
    ) -> Option<Response> {
        match self {
            Self::Filesystem(base_path) => {
                let candidate = filesystem_candidate(base_path, &request_path.segments);
                let hint = candidate.to_string_lossy().into_owned();

                if let Some(encoding) = encoding {
                    let variant = append_suffix(&candidate, encoding.suffix());
                    if let Some(response) =
                        serve_filesystem_file(&variant, &hint, Some(encoding)).await
                    {
                        return Some(response);
                    }
                }
                serve_filesystem_file(&candidate, &hint, None).await
            }
            #[cfg(feature = "embed")]
            Self::Embedded => {
                let candidate = embedded_candidate(&request_path.segments);

                if let Some(encoding) = encoding {
                    let variant = format!("{candidate}{}", encoding.suffix());
                    if let Some(response) =
                        serve_embedded_file(&variant, &candidate, Some(encoding))
                    {
                        return Some(response);
                    }
                }
                serve_embedded_file(&candidate, &candidate, None)
            }
        }
    }

    async fn serve_index(&self, encoding: Option<Precompressed>) -> Option<Response> {
        match self {
            Self::Filesystem(base_path) => {
                let candidate = base_path.join("index.html");
                let hint = candidate.to_string_lossy().into_owned();

                if let Some(encoding) = encoding {
                    let variant = append_suffix(&candidate, encoding.suffix());
                    if let Some(response) =
                        serve_filesystem_file(&variant, &hint, Some(encoding)).await
                    {
                        return Some(response);
                    }
                }
                serve_filesystem_file(&candidate, &hint, None).await
            }
            #[cfg(feature = "embed")]
            Self::Embedded => {
                if let Some(encoding) = encoding {
                    let variant = format!("index.html{}", encoding.suffix());
                    if let Some(response) =
                        serve_embedded_file(&variant, "index.html", Some(encoding))
                    {
                        return Some(response);
                    }
                }
                serve_embedded_file("index.html", "index.html", None)
            }
        }
    }
}

/// 从 `Accept-Encoding` 中挑选可用的预压缩编码，优先 brotli。
fn preferred_precompression(accept_encoding: Option<&str>) -> Option<Precompressed> {
    let header = accept_encoding?.to_ascii_lowercase();
    if accepts_encoding(&header, "br") {
        return Some(Precompressed::Brotli);
    }
    if accepts_encoding(&header, "gzip") {
        return Some(Precompressed::Gzip);
    }
    None
}

/// 是否接受某个编码（支持 `*` 通配，并忽略 `q=0`）。
fn accepts_encoding(header: &str, encoding: &str) -> bool {
    header.split(',').any(|part| {
        let mut pieces = part.trim().split(';');
        let name = pieces.next().unwrap_or("").trim();
        if name != encoding && name != "*" {
            return false;
        }

        let rejected = pieces.any(|parameter| {
            let mut kv = parameter.trim().splitn(2, '=');
            let key = kv.next().unwrap_or("").trim();
            let value = kv.next().unwrap_or("").trim();
            key == "q" && value.parse::<f32>().map(|q| q <= 0.0).unwrap_or(false)
        });
        !rejected
    })
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn resolve_request_path(uri_path: &str) -> Option<FrontendRequestPath> {
    let trimmed = uri_path.trim_start_matches('/');
    let mut segments = Vec::new();

    if !trimmed.is_empty() {
        for component in Path::new(trimmed).components() {
            match component {
                Component::Normal(part) => segments.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                _ => return None,
            }
        }
    }

    let is_resource_like = segments
        .last()
        .and_then(|segment| Path::new(segment).extension())
        .is_some();

    Some(FrontendRequestPath {
        segments,
        is_resource_like,
    })
}

fn filesystem_candidate(base_path: &Path, segments: &[String]) -> PathBuf {
    let mut candidate = base_path.to_path_buf();
    if segments.is_empty() {
        candidate.push("index.html");
        return candidate;
    }

    for segment in segments {
        candidate.push(segment);
    }

    candidate
}

#[cfg(feature = "embed")]
fn embedded_candidate(segments: &[String]) -> String {
    if segments.is_empty() {
        return "index.html".to_string();
    }

    segments.join("/")
}

async fn serve_filesystem_file(
    path: &Path,
    hint: &str,
    encoding: Option<Precompressed>,
) -> Option<Response> {
    if !path.is_file() {
        return None;
    }

    let bytes = tokio::fs::read(path).await.ok()?;
    Some(static_file_response(bytes, hint, encoding))
}

#[cfg(feature = "embed")]
fn serve_embedded_file(
    path: &str,
    hint: &str,
    encoding: Option<Precompressed>,
) -> Option<Response> {
    let file = EMBEDDED_FRONTEND.get_file(path)?;
    Some(static_file_response(file.contents().to_vec(), hint, encoding))
}

fn static_file_response(
    bytes: Vec<u8>,
    path_hint: &str,
    encoding: Option<Precompressed>,
) -> Response {
    let mime = mime_guess::from_path(path_hint).first_or_octet_stream();
    let cache_control = if path_hint.ends_with("index.html") {
        // The SPA shell must be revalidated so new asset hashes are picked up.
        "no-cache"
    } else if path_hint.contains("assets/") {
        // Vite emits content-hashed asset filenames; they are safe to cache
        // aggressively.
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };

    // 显式设置 Content-Length：压缩中间件会把已知长度的 body 包成流式 body，
    // 若不声明长度就会退化为 chunked 传输。
    let content_length = bytes.len();
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CONTENT_LENGTH, content_length)
        .header(header::CACHE_CONTROL, cache_control)
        .header(header::VARY, "accept-encoding");

    if let Some(encoding) = encoding {
        builder = builder.header(header::CONTENT_ENCODING, encoding.header_value());
    }

    builder
        .body(Body::from(bytes))
        .expect("static file response should build")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_encoding_handles_quality_and_wildcards() {
        assert!(accepts_encoding("gzip, br", "br"));
        assert!(accepts_encoding("gzip, br", "gzip"));
        assert!(accepts_encoding("*", "br"));
        assert!(!accepts_encoding("gzip", "br"));
        assert!(!accepts_encoding("br;q=0", "br"));
        assert!(accepts_encoding("br;q=0.5", "br"));
        assert!(!accepts_encoding("identity", "gzip"));
    }

    #[test]
    fn preferred_precompression_prefers_brotli() {
        assert_eq!(
            preferred_precompression(Some("gzip, br")),
            Some(Precompressed::Brotli)
        );
        assert_eq!(
            preferred_precompression(Some("gzip")),
            Some(Precompressed::Gzip)
        );
        assert_eq!(preferred_precompression(Some("identity")), None);
        assert_eq!(preferred_precompression(None), None);
    }
}

#[cfg(all(test, feature = "embed"))]
mod embedded_tests {
    use super::FrontendAssets;
    use axum::http::StatusCode;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn embedded_frontend_serves_root_and_spa_routes() {
        let frontend = FrontendAssets::embedded().expect("embedded frontend should exist");

        let root = frontend.serve("/", Some("gzip")).await;
        assert_eq!(root.status(), StatusCode::OK);
        let root_bytes = root
            .into_body()
            .collect()
            .await
            .expect("root body should collect")
            .to_bytes();
        assert!(!root_bytes.is_empty());

        let spa = frontend.serve("/login", None).await;
        assert_eq!(spa.status(), StatusCode::OK);

        let missing_asset = frontend.serve("/assets/missing.js", None).await;
        assert_eq!(missing_asset.status(), StatusCode::NOT_FOUND);
    }
}
