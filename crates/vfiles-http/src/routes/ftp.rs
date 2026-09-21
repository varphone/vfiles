//! FTP 导入信息：向已登录用户展示连接地址（不含任何密钥）。

use axum::{Json, extract::State};
use serde::Serialize;

use axum_extra::extract::cookie::CookieJar;

use crate::{AppState, error::ApiResult};

/// 路由：`GET /api/files/ftp-info`
pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route("/ftp-info", axum::routing::get(ftp_info))
}

#[derive(Debug, Serialize)]
pub struct FtpInfoResponse {
    pub enabled: bool,
    pub host: String,
    /// 地址来源，前端据此解释「为什么是这个地址」。
    pub host_source: FtpHostSource,
    /// 该地址是否可能被其它机器访问到（回环/通配地址为 false）。
    pub remote_reachable: bool,
    pub port: u16,
    pub passive_ports: PassivePorts,
    pub tls: FtpTlsInfo,
    /// 客户端连接示例（不含口令）。
    pub example_command: Option<String>,
    /// 每个用户只能看到自己的命名空间，这里给出目录映射说明。
    pub path_mapping: String,
}

/// 连接地址的来源，优先级从高到低。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FtpHostSource {
    /// 显式配置了 VFILES_FTP_PASSIVE_HOST（NAT/端口映射场景）
    PassiveHost,
    /// 来自当前请求的 Host 头：客户端就是用这个地址访问到 Web 的
    Request,
    /// 来自 VFILES_HTTP_PUBLIC_BASE_URL
    PublicBaseUrl,
    /// 来自 FTP 绑定地址本身（配置了具体网卡地址时）
    BindAddress,
    /// 通过默认路由探测到的本机网卡地址
    DetectedAddress,
    /// 只能给出回环地址：仅本机可用
    Loopback,
}

#[derive(Debug, Serialize)]
pub struct PassivePorts {
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Serialize)]
pub struct FtpTlsInfo {
    pub enabled: bool,
    pub required: bool,
}

/// `GET /api/files/ftp-info`
///
/// 路由挂载见 `files_router`。
///
/// 需要登录：FTP 连接信息只对有权限的用户可见；返回内容不含口令或证书内容。
pub async fn ftp_info(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<FtpInfoResponse>> {
    // 仅登录用户可读：连接信息（含主机名/端口）不应匿名暴露
    let _ = super::require_auth_user(&state, &jar).await?;

    let request_host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok());

    Ok(Json(build_ftp_info(
        &state.config,
        request_host,
        detected_local_ip(),
    )))
}

/// 通过默认路由探测本机对外地址（`connect` 不发送任何报文，只选路由）。
///
/// 结果缓存一次：同一个进程里网卡地址不会变化，避免每次请求都建 socket。
fn detected_local_ip() -> Option<std::net::IpAddr> {
    static DETECTED: std::sync::OnceLock<Option<std::net::IpAddr>> = std::sync::OnceLock::new();

    *DETECTED.get_or_init(|| {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        // TEST-NET-3（RFC 5737）地址：仅用于选出路由，不会真的发包
        socket.connect("203.0.113.1:9").ok()?;
        match socket.local_addr().ok()?.ip() {
            ip if ip.is_unspecified() || ip.is_loopback() => None,
            ip => Some(ip),
        }
    })
}

/// 由配置构造响应体（纯函数，便于直接测试各种地址来源）。
///
/// `request_host`：当前请求的 `Host` 头；`detected_ip`：本机对外地址探测结果。
pub(crate) fn build_ftp_info(
    config: &vfiles_config::AppConfig,
    request_host: Option<&str>,
    detected_ip: Option<std::net::IpAddr>,
) -> FtpInfoResponse {
    let ftp = &config.ftp;
    let selection = select_connect_host(config, request_host, detected_ip);
    let host = selection.value.clone();

    let example_command = if ftp.enabled {
        let tls_flag = if ftp.tls_cert.is_some() {
            " --ftp-ssl"
        } else {
            ""
        };
        // IPv6 字面量在 URL 中需要方括号
        let authority = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.clone()
        };
        Some(format!(
            "curl{tls_flag} -T 本地文件 ftp://{authority}:{}/目录/",
            ftp.port
        ))
    } else {
        None
    };

    FtpInfoResponse {
        enabled: ftp.enabled,
        host,
        host_source: selection.source,
        remote_reachable: selection.remote_reachable,
        port: ftp.port,
        passive_ports: PassivePorts {
            start: ftp.passive_ports.0,
            end: ftp.passive_ports.1,
        },
        tls: FtpTlsInfo {
            enabled: ftp.tls_cert.is_some(),
            required: ftp.tls_required,
        },
        example_command,
        path_mapping: "登录后 / 即该用户的命名空间根目录".to_string(),
    }
}

/// 地址候选。
#[derive(Debug, Clone)]
struct HostSelection {
    value: String,
    source: FtpHostSource,
    remote_reachable: bool,
}

/// 按「客户端最可能连得上」的顺序挑选要展示的地址。
///
/// 关键点：**远程可达的地址永远优先于回环地址**。默认部署里
/// `VFILES_HTTP_PUBLIC_BASE_URL` 常常是 `http://localhost:3000`，若直接按「配置优先」
/// 就会把 localhost 展示给远程客户端——那是它们连不上的地址。
///
/// 候选顺序：
/// 1. `VFILES_FTP_PASSIVE_HOST`：管理员显式声明的对外地址（NAT/端口映射）；
/// 2. 当前请求的 `Host` 头：客户端正是用它访问到 Web 的（如 `http://192.168.1.5:3000`）；
/// 3. `VFILES_HTTP_PUBLIC_BASE_URL` 的主机名；
/// 4. FTP 绑定地址（配置了具体网卡地址时）；
/// 5. 通过默认路由探测到的本机地址。
///
/// 上面前五者中若只有回环/`localhost`，则退而展示该回环地址，并把
/// `remote_reachable` 置为 `false`，由前端提示「仅本机可访问」。
fn select_connect_host(
    config: &vfiles_config::AppConfig,
    request_host: Option<&str>,
    detected_ip: Option<std::net::IpAddr>,
) -> HostSelection {
    let ftp = &config.ftp;

    let passive_host = ftp
        .passive_host
        .as_deref()
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(|host| candidate(host, FtpHostSource::PassiveHost));

    let from_request = request_host
        .map(strip_port)
        .filter(|host| !host.is_empty() && !is_wildcard(host))
        .map(|host| candidate(host, FtpHostSource::Request));

    let from_public_base = config
        .http
        .public_base_url
        .host_str()
        .filter(|host| !host.is_empty() && !is_wildcard(host))
        .map(|host| candidate(host, FtpHostSource::PublicBaseUrl));

    let bind_host = ftp.host.trim();
    let from_bind = (!bind_host.is_empty() && !is_wildcard(bind_host))
        .then(|| candidate(bind_host, FtpHostSource::BindAddress));

    let from_detected = detected_ip.map(|ip| HostSelection {
        value: ip.to_string(),
        source: FtpHostSource::DetectedAddress,
        remote_reachable: !ip.is_loopback(),
    });

    let mut local_only: Option<HostSelection> = None;
    for candidate in [
        passive_host,
        from_request,
        from_public_base,
        from_bind,
        from_detected,
    ]
    .into_iter()
    .flatten()
    {
        if candidate.remote_reachable {
            return candidate;
        }
        // 回环地址只作为兜底：先记住，继续找有没有远程可达的候选
        local_only.get_or_insert(candidate);
    }

    local_only.unwrap_or(HostSelection {
        value: "127.0.0.1".to_string(),
        source: FtpHostSource::Loopback,
        remote_reachable: false,
    })
}

fn candidate(host: &str, source: FtpHostSource) -> HostSelection {
    let value = normalize_host(host);
    HostSelection {
        remote_reachable: !is_local_only(&value),
        value,
        source,
    }
}

/// 去掉 `Host` 头里的端口与 IPv6 方括号，得到可展示的主机名。
fn strip_port(raw: &str) -> &str {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('[') {
        // [::1]:3000 → ::1
        return rest.split(']').next().unwrap_or(rest);
    }
    // 注意：IPv6 字面量本身含冒号，只有「一个冒号」时才可能是 host:port
    match raw.matches(':').count() {
        1 => raw.split(':').next().unwrap_or(raw),
        _ => raw,
    }
}

fn normalize_host(raw: &str) -> String {
    strip_port(raw).to_string()
}

fn is_wildcard(host: &str) -> bool {
    let host = host.trim();
    host == "0.0.0.0" || host == "::" || host == "[::]" || host.is_empty()
}

/// 回环地址或 localhost：仅本机可访问。
fn is_local_only(host: &str) -> bool {
    let host = host.trim().trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host == "::1" {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn config() -> vfiles_config::AppConfig {
        vfiles_config::ConfigLoader::load().expect("config should load")
    }

    fn ip(value: &str) -> IpAddr {
        value.parse().expect("ip should parse")
    }

    #[test]
    fn reports_enabled_default_configuration() {
        let config = config();
        let info = build_ftp_info(&config, None, None);

        assert!(info.enabled, "FTP 默认开启");
        assert_eq!(info.port, 2121);
        assert_eq!(info.passive_ports.start, 50_000);
        assert_eq!(info.passive_ports.end, 50_100);
        assert!(!info.tls.enabled);
        assert!(info.example_command.is_some(), "开启时给出示例命令");
        assert!(
            !info
                .example_command
                .unwrap_or_default()
                .contains("--ftp-ssl"),
            "未启用 TLS 时示例命令不应带 --ftp-ssl"
        );
    }

    #[test]
    fn reports_disabled_configuration_without_example() {
        let mut config = config();
        config.ftp.enabled = false;
        let info = build_ftp_info(&config, None, None);

        assert!(!info.enabled);
        assert!(info.example_command.is_none(), "关闭时不给出连接示例");
    }

    #[test]
    fn uses_passive_host_and_tls_flags_when_configured() {
        let mut config = config();
        config.ftp.passive_host = Some("files.example.com".to_string());
        config.ftp.tls_cert = Some("/etc/vfiles/cert.pem".to_string());
        config.ftp.tls_key = Some("/etc/vfiles/key.pem".to_string());
        config.ftp.tls_required = true;
        let info = build_ftp_info(&config, Some("192.168.1.5:3000"), None);

        assert_eq!(info.host, "files.example.com");
        assert_eq!(info.host_source, FtpHostSource::PassiveHost);
        assert!(info.remote_reachable);
        assert!(info.tls.enabled);
        assert!(info.tls.required);
        assert!(
            info.example_command
                .unwrap_or_default()
                .contains("--ftp-ssl"),
            "启用 TLS 时示例命令应带 --ftp-ssl"
        );
    }

    #[test]
    fn prefers_the_address_the_client_actually_used() {
        let config = config();
        // 默认 public_base_url 是 localhost；客户端用局域网地址访问时应展示该地址
        let info = build_ftp_info(&config, Some("192.168.1.5:3000"), Some(ip("10.0.0.7")));

        assert_eq!(info.host, "192.168.1.5");
        assert_eq!(info.host_source, FtpHostSource::Request);
        assert!(info.remote_reachable, "局域网地址对远程客户端可用");
        assert!(
            info.example_command
                .unwrap_or_default()
                .contains("ftp://192.168.1.5:2121/"),
            "示例命令应使用同一个地址"
        );
    }

    #[test]
    fn ignores_wildcard_and_never_shows_zero_address() {
        let mut config = config();
        config.ftp.host = "0.0.0.0".to_string();
        let info = build_ftp_info(&config, Some("0.0.0.0:3000"), Some(ip("192.168.1.20")));

        assert_eq!(info.host, "192.168.1.20", "通配绑定地址不能展示给客户端");
        assert_eq!(info.host_source, FtpHostSource::DetectedAddress);
        assert!(info.remote_reachable);
    }

    #[test]
    fn detects_local_only_addresses_and_flags_them() {
        let config = config();
        // Host 头是 localhost ⇒ 展示它，但标记为仅本机可达
        let info = build_ftp_info(&config, Some("localhost:3000"), None);
        assert_eq!(info.host, "localhost");
        assert_eq!(info.host_source, FtpHostSource::Request);
        assert!(!info.remote_reachable, "localhost 不应被标记为远程可达");

        // 回环 IP 同理
        let info = build_ftp_info(&config, Some("127.0.0.1:3000"), None);
        assert!(!info.remote_reachable);
        assert!(super::is_local_only("127.0.0.1"));
        assert!(super::is_local_only("[::1]"));
        assert!(!super::is_local_only("192.168.1.5"));
    }

    #[test]
    fn falls_back_to_public_base_url_then_bind_address() {
        let mut public_base = config();
        public_base.http.public_base_url =
            url::Url::parse("https://files.example.com").expect("url");
        let info = build_ftp_info(&public_base, None, Some(ip("10.0.0.7")));
        assert_eq!(info.host, "files.example.com");
        assert_eq!(info.host_source, FtpHostSource::PublicBaseUrl);

        // public_base_url 是回环时继续向后找：绑定地址（具体网卡地址）优先于探测地址
        let mut loopback_base = config();
        loopback_base.http.public_base_url = url::Url::parse("http://127.0.0.1:3000").expect("url");
        loopback_base.ftp.host = "192.168.1.9".to_string();
        let info = build_ftp_info(&loopback_base, None, None);
        assert_eq!(info.host, "192.168.1.9");
        assert_eq!(info.host_source, FtpHostSource::BindAddress);
        assert!(info.remote_reachable);
    }

    #[test]
    fn formats_ipv6_hosts_with_brackets_in_the_example() {
        let mut config = config();
        config.ftp.host = "fd00::5".to_string();
        config.http.public_base_url = url::Url::parse("http://[fd00::5]:3000").expect("url");
        let info = build_ftp_info(&config, Some("[fd00::5]:3000"), None);

        assert_eq!(info.host, "fd00::5", "展示时去掉方括号");
        let command = info.example_command.clone().unwrap_or_default();
        assert!(
            command.contains("ftp://[fd00::5]:2121/"),
            "IPv6 示例命令需要方括号，实际: {command:?}"
        );
    }
}
