//! HTTP middleware and shared request guards for VFiles.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, SemaphorePermit};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderName, HeaderValue, Method, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use vfiles_app::LoginRateLimitBlock;

const CONTENT_SECURITY_POLICY: &str = concat!(
    "default-src 'self'; ",
    "base-uri 'self'; ",
    "connect-src 'self'; ",
    "form-action 'self'; ",
    "frame-ancestors 'self'; ",
    "frame-src 'self' blob:; ",
    "img-src 'self' blob: data:; ",
    "media-src 'self' blob:; ",
    "object-src 'none'; ",
    "script-src 'self'; ",
    "style-src 'self' 'unsafe-inline'; ",
    "worker-src 'self' blob:"
);

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
const RESOLVED_CLIENT_IP_HEADER: &str = "x-vfiles-client-ip";
const MAX_FIXED_WINDOW_COUNTERS: usize = 50_000;
/// Bound active API handler work so overload fails fast instead of building an
/// unbounded queue of tasks waiting on SQLite and storage operations.
pub(crate) const MAX_CONCURRENT_API_REQUESTS: usize = 128;
static API_REQUEST_PERMITS: Semaphore = Semaphore::const_new(MAX_CONCURRENT_API_REQUESTS);

fn try_acquire_api_request_permit(semaphore: &Semaphore) -> Option<SemaphorePermit<'_>> {
    semaphore.try_acquire().ok()
}

pub async fn api_request_admission_middleware(req: Request, next: Next) -> Response {
    let Some(_permit) = try_acquire_api_request_permit(&API_REQUEST_PERMITS) else {
        return crate::error::ApiError::rate_limited(1).into_response();
    };

    next.run(req).await
}

fn normalize_request_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
    {
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

/// 访问令牌专用 cookie 名：把 `Authorization: Bearer` 复制到 cookie，
/// 这样既有的 `CookieJar` 鉴权路径（以及所有现有处理函数）无需改动即可支持令牌。
pub const ACCESS_TOKEN_COOKIE: &str = "vfiles_token";

/// 把 `Authorization: Bearer <token>` 归一化成内部 cookie。
///
/// 只在请求确实带了 Bearer 且看起来是访问令牌时改写；否则原样放行。
pub async fn bearer_token_middleware(mut req: Request, next: Next) -> Response {
    let bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
        })
        .filter(|token| token.starts_with("vfat_"))
        .map(str::to_string);

    if let Some(token) = bearer
        && let Ok(value) =
            axum::http::HeaderValue::from_str(&format!("{ACCESS_TOKEN_COOKIE}={token}"))
    {
        req.headers_mut().append(axum::http::header::COOKIE, value);
    }

    next.run(req).await
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
    headers
        .entry(HeaderName::from_static("content-security-policy"))
        .or_insert(HeaderValue::from_static(CONTENT_SECURITY_POLICY));

    response
}

/// Reject browser write requests from origins that are not configured for this API.
///
/// CORS does not stop HTML forms from submitting `multipart/form-data`; this guard
/// covers those requests as well as cross-origin fetches. Requests without browser
/// origin metadata remain available to command-line clients.
pub async fn write_origin_guard_middleware(
    State(allowed_origins): State<std::sync::Arc<Vec<String>>>,
    req: Request,
    next: Next,
) -> Response {
    if !is_write_origin_allowed(req.method(), req.headers(), &allowed_origins) {
        return crate::error::ApiError::forbidden("Cross-origin write requests are not allowed")
            .into_response();
    }

    next.run(req).await
}

fn is_write_origin_allowed(
    method: &Method,
    headers: &axum::http::HeaderMap,
    allowed_origins: &[String],
) -> bool {
    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        return true;
    }

    let mut origins = headers.get_all(header::ORIGIN).iter();
    if let Some(origin) = origins.next() {
        return origins.next().is_none()
            && origin.to_str().is_ok_and(|origin| {
                allowed_origins
                    .iter()
                    .any(|allowed| origin.eq_ignore_ascii_case(allowed))
            });
    }

    // `same-site` is insufficient because an untrusted sibling domain shares cookies.
    // When Origin is absent, only an explicit same-origin Fetch Metadata value is safe.
    let mut fetch_sites = headers.get_all("sec-fetch-site").iter();
    if let Some(fetch_site) = fetch_sites.next() {
        return fetch_sites.next().is_none()
            && fetch_site
                .to_str()
                .is_ok_and(|value| value.eq_ignore_ascii_case("same-origin"));
    }

    true
}

/// Read only the internal value installed by `client_ip_middleware`.
pub(crate) fn client_ip_from_headers(headers: &axum::http::HeaderMap) -> String {
    headers
        .get(RESOLVED_CLIENT_IP_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "unknown".to_string())
}

/// Remove caller-supplied IP headers and replace them with a value derived from
/// the socket peer. Forwarded headers are consulted only when that peer is an
/// explicitly trusted reverse proxy.
pub async fn client_ip_middleware(
    State(trusted_proxy_ips): State<std::sync::Arc<Vec<IpAddr>>>,
    mut req: Request,
    next: Next,
) -> Response {
    let peer_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip());
    let client_ip = resolve_client_ip(peer_ip, req.headers(), &trusted_proxy_ips);

    for name in [
        "x-forwarded-for",
        "cf-connecting-ip",
        "x-real-ip",
        "x-client-ip",
        RESOLVED_CLIENT_IP_HEADER,
    ] {
        req.headers_mut().remove(name);
    }
    if let Some(client_ip) = client_ip
        && let Ok(value) = HeaderValue::from_str(&client_ip.to_string())
    {
        req.headers_mut()
            .insert(HeaderName::from_static(RESOLVED_CLIENT_IP_HEADER), value);
    }

    next.run(req).await
}

fn resolve_client_ip(
    peer_ip: Option<IpAddr>,
    headers: &axum::http::HeaderMap,
    trusted_proxy_ips: &[IpAddr],
) -> Option<IpAddr> {
    let peer_ip = peer_ip?;
    if !trusted_proxy_ips.contains(&peer_ip) {
        return Some(peer_ip);
    }

    if let Some(mut chain) = forwarded_for_chain(headers) {
        let mut client_ip = peer_ip;
        while let Some(hop) = chain.pop() {
            if !trusted_proxy_ips.contains(&client_ip) {
                break;
            }
            client_ip = hop;
        }
        return Some(client_ip);
    }

    for name in ["cf-connecting-ip", "x-real-ip", "x-client-ip"] {
        if let Some(client_ip) = single_forwarded_ip(headers, name) {
            return Some(client_ip);
        }
    }

    Some(peer_ip)
}

fn forwarded_for_chain(headers: &axum::http::HeaderMap) -> Option<Vec<IpAddr>> {
    let values = headers.get_all("x-forwarded-for");
    let mut chain = Vec::new();
    for value in values {
        let value = value.to_str().ok()?;
        for hop in value.split(',') {
            chain.push(parse_forwarded_ip(hop.trim())?);
        }
    }
    (!chain.is_empty()).then_some(chain)
}

fn single_forwarded_ip(headers: &axum::http::HeaderMap, name: &'static str) -> Option<IpAddr> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    parse_forwarded_ip(value.trim())
}

fn parse_forwarded_ip(value: &str) -> Option<IpAddr> {
    if let Ok(ip) = value.parse::<IpAddr>() {
        return Some(ip);
    }
    if let Ok(address) = value.parse::<SocketAddr>() {
        return Some(address.ip());
    }
    value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.parse::<IpAddr>().ok())
}

#[derive(Debug)]
struct FixedWindowCounter {
    requests: u32,
    reset_at: Instant,
}

#[derive(Debug, Default)]
struct FixedWindowState {
    counters: HashMap<[u8; 32], FixedWindowCounter>,
}

#[derive(Debug, Default)]
pub struct FixedWindowLimiter {
    state: Mutex<FixedWindowState>,
}

impl FixedWindowLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, FixedWindowState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn key_digest(key: &str) -> [u8; 32] {
        Sha256::digest(key.as_bytes()).into()
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
        let key = Self::key_digest(key);
        let mut state = self.lock_state();

        if let Some(counter) = state.counters.get_mut(&key) {
            if now >= counter.reset_at {
                counter.requests = 1;
                counter.reset_at = now + window;
                return None;
            }
            if counter.requests >= max_requests {
                let retry_after = counter.reset_at.saturating_duration_since(now);
                return Some(LoginRateLimitBlock {
                    retry_after_secs: ceil_duration_seconds(retry_after),
                });
            }
            counter.requests = counter.requests.saturating_add(1);
            return None;
        }

        // These counters are best-effort. Evict one entry at capacity so unique
        // attacker-controlled keys cannot grow or repeatedly scan the table.
        if state.counters.len() >= MAX_FIXED_WINDOW_COUNTERS
            && let Some(victim) = state.counters.keys().next().copied()
        {
            state.counters.remove(&victim);
        }
        state.counters.insert(
            key,
            FixedWindowCounter {
                requests: 1,
                reset_at: now + window,
            },
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use axum::http::{HeaderMap, HeaderValue, Method, header};
    use tokio::sync::Semaphore;

    use super::{
        FixedWindowLimiter, MAX_FIXED_WINDOW_COUNTERS, client_ip_from_headers,
        is_write_origin_allowed, normalize_request_id, resolve_client_ip,
        try_acquire_api_request_permit,
    };

    fn ip(value: &str) -> IpAddr {
        value.parse().expect("IP address should parse")
    }

    #[test]
    fn request_ids_reject_whitespace_and_keep_common_trace_tokens() {
        assert_eq!(
            normalize_request_id("proxy-req_01.2-ab"),
            Some("proxy-req_01.2-ab".to_string())
        );
        assert_eq!(normalize_request_id("proxy request"), None);
        assert_eq!(normalize_request_id("proxy\trequest"), None);
    }

    #[test]
    fn api_request_admission_fails_fast_at_capacity_and_releases_slots() {
        let semaphore = Semaphore::new(2);
        let first = try_acquire_api_request_permit(&semaphore).expect("first permit");
        let second = try_acquire_api_request_permit(&semaphore).expect("second permit");

        assert!(try_acquire_api_request_permit(&semaphore).is_none());

        drop(first);
        assert!(try_acquire_api_request_permit(&semaphore).is_some());
        drop(second);
    }

    #[test]
    fn client_ip_consumers_ignore_untrusted_header_names() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.99"));
        headers.insert(
            "cf-connecting-ip",
            HeaderValue::from_static("198.51.100.99"),
        );

        assert_eq!(client_ip_from_headers(&headers), "unknown");

        headers.insert("x-vfiles-client-ip", HeaderValue::from_static("192.0.2.10"));
        assert_eq!(client_ip_from_headers(&headers), "192.0.2.10");
    }

    #[test]
    fn ignores_forwarded_headers_from_untrusted_peers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.99"));

        assert_eq!(
            resolve_client_ip(Some(ip("192.0.2.10")), &headers, &[ip("10.0.0.2")]),
            Some(ip("192.0.2.10"))
        );
    }

    #[test]
    fn resolves_forwarded_chain_from_the_nearest_untrusted_hop() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.99, 198.51.100.20, 10.0.0.1"),
        );
        let trusted_proxies = [ip("10.0.0.1"), ip("10.0.0.2")];

        assert_eq!(
            resolve_client_ip(Some(ip("10.0.0.2")), &headers, &trusted_proxies),
            Some(ip("198.51.100.20"))
        );
    }

    #[test]
    fn falls_back_to_the_peer_when_a_trusted_proxy_sends_malformed_forwarding_data() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("attacker-input"),
        );
        headers.insert("x-real-ip", HeaderValue::from_static("also-invalid"));

        assert_eq!(
            resolve_client_ip(Some(ip("10.0.0.2")), &headers, &[ip("10.0.0.2")]),
            Some(ip("10.0.0.2"))
        );
    }

    #[test]
    fn fixed_window_limiter_blocks_after_limit() {
        let limiter = FixedWindowLimiter::new();
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

    #[test]
    fn fixed_window_limiter_bounds_unique_key_storage() {
        let limiter = FixedWindowLimiter::new();
        let window = std::time::Duration::from_secs(60);

        for index in 0..=MAX_FIXED_WINDOW_COUNTERS {
            let key = format!("share:{index}:127.0.0.1");
            limiter.check_and_record(60, window, &key);
        }

        let state = limiter.lock_state();
        assert_eq!(state.counters.len(), MAX_FIXED_WINDOW_COUNTERS);
        assert!(state.counters.keys().all(|key| key.len() == 32));
    }

    #[test]
    fn write_origin_guard_rejects_untrusted_and_sibling_origins() {
        let allowed_origins = vec!["https://files.example.test".to_string()];
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.test"),
        );
        headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));

        assert!(!is_write_origin_allowed(
            &Method::POST,
            &headers,
            &allowed_origins
        ));

        headers.remove(header::ORIGIN);
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-site"));
        assert!(!is_write_origin_allowed(
            &Method::POST,
            &headers,
            &allowed_origins
        ));
    }

    #[test]
    fn write_origin_guard_allows_configured_and_non_browser_requests() {
        let allowed_origins = vec!["https://files.example.test".to_string()];
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("HTTPS://FILES.EXAMPLE.TEST"),
        );
        assert!(is_write_origin_allowed(
            &Method::POST,
            &headers,
            &allowed_origins
        ));

        headers.clear();
        assert!(is_write_origin_allowed(
            &Method::PUT,
            &headers,
            &allowed_origins
        ));

        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert!(is_write_origin_allowed(
            &Method::DELETE,
            &headers,
            &allowed_origins
        ));
        assert!(is_write_origin_allowed(
            &Method::GET,
            &HeaderMap::new(),
            &allowed_origins
        ));
    }
}
