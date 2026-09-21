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
    let ftp = &state.config.ftp;
    // 对外通告的地址优先（NAT 场景），否则回退到 public_base_url 的主机名
    let host = ftp
        .passive_host
        .clone()
        .or_else(|| {
            state
                .config
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

    Ok(Json(FtpInfoResponse {
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
    }))
}
