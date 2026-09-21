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
    pub port: u16,
    pub passive_ports: PassivePorts,
    pub tls: FtpTlsInfo,
    /// 客户端连接示例（不含口令）。
    pub example_command: Option<String>,
    /// 每个用户只能看到自己的命名空间，这里给出目录映射说明。
    pub path_mapping: String,
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
) -> ApiResult<Json<FtpInfoResponse>> {
    // 仅登录用户可读：连接信息（含主机名/端口）不应匿名暴露
    let _ = super::require_auth_user(&state, &jar).await?;
    Ok(Json(build_ftp_info(&state.config)))
}

/// 由配置构造响应体（纯函数，便于直接测试启用/关闭两种形态）。
pub(crate) fn build_ftp_info(config: &vfiles_config::AppConfig) -> FtpInfoResponse {
    let ftp = &config.ftp;
    // 对外通告的地址优先（NAT 场景），否则回退到 public_base_url 的主机名
    let host = ftp
        .passive_host
        .clone()
        .or_else(|| {
            config
                .http
                .public_base_url
                .host_str()
                .map(|value| value.to_string())
        })
        .unwrap_or_else(|| ftp.host.clone());

    let example_command = if ftp.enabled {
        let tls_flag = if ftp.tls_cert.is_some() {
            " --ftp-ssl"
        } else {
            ""
        };
        Some(format!(
            "curl{tls_flag} -T 本地文件 ftp://{host}:{}/目录/",
            ftp.port
        ))
    } else {
        None
    };

    FtpInfoResponse {
        enabled: ftp.enabled,
        host,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> vfiles_config::AppConfig {
        vfiles_config::ConfigLoader::load().expect("config should load")
    }

    #[test]
    fn reports_enabled_default_configuration() {
        let config = config();
        let info = build_ftp_info(&config);

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
        let info = build_ftp_info(&config);

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
        let info = build_ftp_info(&config);

        assert_eq!(info.host, "files.example.com");
        assert!(info.tls.enabled);
        assert!(info.tls.required);
        assert!(
            info.example_command
                .unwrap_or_default()
                .contains("--ftp-ssl"),
            "启用 TLS 时示例命令应带 --ftp-ssl"
        );
    }
}
