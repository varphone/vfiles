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

    pub async fn serve(&self, uri_path: &str) -> Response {
        let Some(request_path) = resolve_request_path(uri_path) else {
            return StatusCode::NOT_FOUND.into_response();
        };

        if let Some(response) = self.serve_requested_path(&request_path).await {
            return response;
        }

        if request_path.is_resource_like {
            return StatusCode::NOT_FOUND.into_response();
        }

        self.serve_index()
            .await
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
    }

    async fn serve_requested_path(&self, request_path: &FrontendRequestPath) -> Option<Response> {
        match self {
            Self::Filesystem(base_path) => {
                let candidate = filesystem_candidate(base_path, &request_path.segments);
                serve_filesystem_file(&candidate).await
            }
            #[cfg(feature = "embed")]
            Self::Embedded => {
                let candidate = embedded_candidate(&request_path.segments);
                serve_embedded_file(&candidate)
            }
        }
    }

    async fn serve_index(&self) -> Option<Response> {
        match self {
            Self::Filesystem(base_path) => {
                serve_filesystem_file(&base_path.join("index.html")).await
            }
            #[cfg(feature = "embed")]
            Self::Embedded => serve_embedded_file("index.html"),
        }
    }
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

async fn serve_filesystem_file(path: &Path) -> Option<Response> {
    if !path.is_file() {
        return None;
    }

    let bytes = tokio::fs::read(path).await.ok()?;
    Some(static_file_response(
        Body::from(bytes),
        path.to_string_lossy().as_ref(),
    ))
}

#[cfg(feature = "embed")]
fn serve_embedded_file(path: &str) -> Option<Response> {
    let file = EMBEDDED_FRONTEND.get_file(path)?;
    Some(static_file_response(
        Body::from(file.contents().to_vec()),
        path,
    ))
}

fn static_file_response(body: Body, path_hint: &str) -> Response {
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

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache_control)
        .body(body)
        .expect("static file response should build")
}

#[cfg(all(test, feature = "embed"))]
mod tests {
    use super::FrontendAssets;
    use axum::http::StatusCode;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn embedded_frontend_serves_root_and_spa_routes() {
        let frontend = FrontendAssets::embedded().expect("embedded frontend should exist");

        let root = frontend.serve("/").await;
        assert_eq!(root.status(), StatusCode::OK);
        let root_bytes = root
            .into_body()
            .collect()
            .await
            .expect("root body should collect")
            .to_bytes();
        assert!(!root_bytes.is_empty());

        let spa = frontend.serve("/login").await;
        assert_eq!(spa.status(), StatusCode::OK);

        let missing_asset = frontend.serve("/assets/missing.js").await;
        assert_eq!(missing_asset.status(), StatusCode::NOT_FOUND);
    }
}
