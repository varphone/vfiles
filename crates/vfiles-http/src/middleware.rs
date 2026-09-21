//! HTTP middleware and shared request guards for VFiles.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use vfiles_app::LoginRateLimitBlock;

/// 登录限流实现在 `vfiles-app`（HTTP 与 FTP 共用），这里为既有调用点重新导出。
pub use vfiles_app::{LoginAttemptLimiter, RateLimitPolicy};

/// 把 HTTP 配置里的登录限流参数转成共用策略。
///
/// 用自由函数而不是 `From` 实现：`LoginRateLimitConfig` 与 `RateLimitPolicy`
/// 都定义在本 crate 之外，无法实现外部 trait 的转换。
pub fn login_rate_limit_policy(config: &vfiles_config::LoginRateLimitConfig) -> RateLimitPolicy {
    RateLimitPolicy {
        enabled: config.enabled,
        window_ms: config.window_ms,
        max_attempts: config.max_attempts,
    }
}

/// 向上取整到秒（固定窗口重试提示用）。
fn ceil_duration_seconds(duration: Duration) -> u64 {
    let secs = duration.as_secs();
    if duration.subsec_nanos() == 0 {
        secs
    } else {
        secs.saturating_add(1)
    }
}

tokio::task_local! {
    pub static REQUEST_ID: String;
}

#[derive(Debug, Clone)]
pub struct RequestId(pub String);

const REQUEST_ID_HEADER: &str = "x-request-id";

fn normalize_request_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return None;
    }
    Some(value.to_string())
}

pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let request_id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_request_id)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    req.extensions_mut().insert(RequestId(request_id.clone()));

    let mut response = REQUEST_ID.scope(request_id.clone(), next.run(req)).await;

    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }

    response
}

pub async fn security_headers_middleware(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers
        .entry(HeaderName::from_static("x-content-type-options"))
        .or_insert(HeaderValue::from_static("nosniff"));
    headers
        .entry(HeaderName::from_static("x-frame-options"))
        .or_insert(HeaderValue::from_static("SAMEORIGIN"));
    headers
        .entry(HeaderName::from_static("referrer-policy"))
        .or_insert(HeaderValue::from_static("strict-origin-when-cross-origin"));
    headers
        .entry(HeaderName::from_static("permissions-policy"))
        .or_insert(HeaderValue::from_static(
            "camera=(), microphone=(), geolocation=()",
        ));

    response
}

pub(crate) fn client_ip_from_headers(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next().map(str::trim))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            ["cf-connecting-ip", "x-real-ip", "x-client-ip"]
                .iter()
                .find_map(|header_name| {
                    headers
                        .get(*header_name)
                        .and_then(|value| value.to_str().ok())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug)]
struct FixedWindowCounter {
    requests: u32,
    reset_at: Instant,
}

#[derive(Debug, Default)]
pub struct FixedWindowLimiter {
    counters: Mutex<HashMap<String, FixedWindowCounter>>,
}

impl FixedWindowLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_counters(&self) -> std::sync::MutexGuard<'_, HashMap<String, FixedWindowCounter>> {
        self.counters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn check_and_record(
        &self,
        max_requests: u32,
        window: Duration,
        key: &str,
    ) -> Option<LoginRateLimitBlock> {
        if max_requests == 0 {
            return Some(LoginRateLimitBlock {
                retry_after_secs: 1,
            });
        }

        let now = Instant::now();
        let mut counters = self.lock_counters();

        if counters.len() >= 50_000 {
            counters.retain(|_, counter| now < counter.reset_at);
        }

        match counters.get_mut(key) {
            Some(counter) if now >= counter.reset_at => {
                counter.requests = 1;
                counter.reset_at = now + window;
                None
            }
            Some(counter) if counter.requests >= max_requests => {
                let retry_after = counter.reset_at.saturating_duration_since(now);
                Some(LoginRateLimitBlock {
                    retry_after_secs: ceil_duration_seconds(retry_after),
                })
            }
            Some(counter) => {
                counter.requests = counter.requests.saturating_add(1);
                None
            }
            None => {
                counters.insert(
                    key.to_string(),
                    FixedWindowCounter {
                        requests: 1,
                        reset_at: now + window,
                    },
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fixed_window_limiter_blocks_after_limit() {
        let limiter = super::FixedWindowLimiter::new();
        let window = std::time::Duration::from_secs(60);

        assert!(
            limiter
                .check_and_record(2, window, "share:a:127.0.0.1")
                .is_none()
        );
        assert!(
            limiter
                .check_and_record(2, window, "share:a:127.0.0.1")
                .is_none()
        );
        let block = limiter
            .check_and_record(2, window, "share:a:127.0.0.1")
            .expect("limiter should block after limit");
        assert!(block.retry_after_secs >= 1);
        assert!(
            limiter
                .check_and_record(2, window, "share:b:127.0.0.1")
                .is_none()
        );
    }
}
