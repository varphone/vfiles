use camino::Utf8PathBuf;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::{env, path::Path};
use url::Url;

use vfiles_domain::FeatureMatrix;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid cookie secret")]
    InvalidCookieSecret,
    #[error("Invalid storage root")]
    InvalidStorageRoot,
    #[error("Configuration load error: {0}")]
    LoadError(String),
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub http: HttpConfig,
    pub storage: StorageConfig,
    pub auth: AuthConfig,
    pub mail: MailConfig,
    pub search: SearchConfig,
    pub limits: LimitsConfig,
    pub maintenance: MaintenanceConfig,
    pub ftp: FtpConfig,
    pub webdav: WebdavConfig,
    /// S3 兼容 API（round 2 ✗ 新协议面 = **默认关显式启用**（`VFILES_S3_ENABLED=true`）✗
    /// 无键 = 运行时随机生成 + warn 打印（零配置试用 ✓ 生产 env 固定 ✓）。
    pub s3: S3Config,
    /// rsync 协议 daemon（round 3 ✗ 新协议面 = **默认关显式启用**（`VFILES_RSYNC_ENABLED=true`）
    /// ✗ 只读匿名模块首版（secrets 密码 = r4 债））。
    pub rsync: RsyncConfig,
    pub features: FeatureMatrix,
}

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
    pub public_base_url: Url,
    pub cookie_secret: SecretString,
    pub cookie_secure_override: Option<bool>,
    pub cors_allowed_origins: Vec<String>,
    pub cors_allow_any_origin: bool,
}

impl HttpConfig {
    pub fn cookie_secure(&self) -> bool {
        self.cookie_secure_override
            .unwrap_or_else(|| self.public_base_url.scheme().eq_ignore_ascii_case("https"))
    }

    pub fn effective_cors_allowed_origins(&self) -> Vec<String> {
        if self.cors_allow_any_origin {
            return Vec::new();
        }

        if self.cors_allowed_origins.is_empty() {
            return vec![self.public_base_url.origin().ascii_serialization()];
        }

        self.cors_allowed_origins.clone()
    }
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub root: Utf8PathBuf,
    pub database_path: Utf8PathBuf,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub allow_register: bool,
    pub session_ttl_seconds: u64,
    pub login_rate_limit: LoginRateLimitConfig,
    pub password_reset_ttl_seconds: u64,
    pub email_login_code_ttl_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct LoginRateLimitConfig {
    pub enabled: bool,
    pub window_ms: u64,
    pub max_attempts: u32,
}

#[derive(Debug, Clone)]
pub struct MailConfig {
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<SecretString>,
    pub from_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub enabled: bool,
    pub max_results: usize,
}

/// FTP 批量导入配置。
///
/// 默认关闭：FTP 是明文协议，需要运维显式开启（并推荐同时配置 FTPS）。
#[derive(Debug, Clone)]
pub struct FtpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    /// 被动模式端口段（含两端）。
    pub passive_ports: (u16, u16),
    /// 对外通告的被动地址（NAT/端口映射场景），支持 IP 或域名。
    pub passive_host: Option<String>,
    pub max_connections: u32,
    pub idle_timeout_seconds: u64,
    /// 允许登录的角色（空表示不限制）。
    pub allowed_roles: Vec<String>,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    pub tls_required: bool,
    /// 快照提交策略：`batch`（默认）/ `per-file` / `off`。
    pub snapshot_mode: String,
    /// `batch` 模式下每累积多少个文件提交一次快照。
    pub snapshot_flush_files: u32,
}

/// WebDAV（r109b ✓ 用户令「默认开启」✓ 与 FtpConfig 同构极简）。
///
/// **默认开启**（`enabled: true` ⚠️ 依用户令）+ **auth 强制防御**（未开认证即 Err ✓
/// 默认开也安全 ✓ 语义差 = FTP `(None,false)→false`；WebDAV `(None,false)→Err` ✓）。
/// per-user ns / bin 挂载 / PUT = r109c 预注明。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavConfig {
    /// **默认开启**（用户令 ✓ 显式 `VFILES_WEBDAV_ENABLED=false` 可关）。
    #[serde(default = "webdav_default_enabled")]
    pub enabled: bool,
    #[serde(default = "webdav_default_host")]
    pub host: String,
    #[serde(default = "webdav_default_port")]
    pub port: u16,
    /// 共端口模式（r-new ✗ 未显式设 `VFILES_WEBDAV_PORT` = 挂主端口 mount 路径 ✓
    /// 显式设 PORT = 独立端口（现行为零回归回退））。
    #[serde(default = "webdav_default_embedded")]
    pub embedded: bool,
    /// 嵌入挂载路径（standalone 时忽略 ✗ 默认 `/dav`，可空 = 根挂载高级项）。
    #[serde(default = "webdav_default_mount")]
    pub mount_path: String,
}

fn webdav_default_enabled() -> bool {
    true
}

fn webdav_default_host() -> String {
    "0.0.0.0".to_string()
}

fn webdav_default_port() -> u16 {
    18080
}

fn webdav_default_embedded() -> bool {
    true
}

fn webdav_default_mount() -> String {
    "/dav".to_string()
}

/// S3 兼容 API 配置（对称 webdav env 形 ✓）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    /// **默认关**（新协议面 = 显式启用原则 ✗ r2 交付注：`VFILES_S3_ENABLED=true` 开）。
    #[serde(default = "s3_default_enabled")]
    pub enabled: bool,
    /// 专用端口（path-style 与前端根语义冲突 ✗ MinIO 9000 惯例）。
    #[serde(default = "s3_default_port")]
    pub port: u16,
    /// Access Key（空 = 运行时随机 + warn 打印）。
    #[serde(default)]
    pub access_key: String,
    /// Secret Key（空 = 同上成对随机）。
    #[serde(default)]
    pub secret_key: String,
    /// 额外汇率对（逗号分隔 `access:secret` ✗ 多客户端/轮换；与上面单对并存）。
    #[serde(default)]
    pub credentials: String,
}

fn s3_default_enabled() -> bool {
    false
}

fn s3_default_port() -> u16 {
    9000
}

/// rsync daemon 配置（对称 S3 先例三变量 ✗ rsync://host/module 形 = 单模块匿名只读）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsyncConfig {
    /// **默认关**（新协议面显式启用原则 ✗ r3 交付注：`VFILES_RSYNC_ENABLED=true` 开）。
    #[serde(default = "rsync_default_enabled")]
    pub enabled: bool,
    /// daemon 专用端口（rsync 生态默认 873）。
    #[serde(default = "rsync_default_port")]
    pub port: u16,
    /// 模块名（`rsync://host/<module>` 的 <module> ✗ 单模块 = 默认 ns 根）。
    #[serde(default = "rsync_default_module")]
    pub module: String,
    /// 模块是否**可写**（r8 ✗ push 需显式开启；false = 只读、拒收 push）。
    #[serde(default)]
    pub writable: bool,
    /// 允许的用户（逗号分隔 ✗ 空 = 匿名；支持 `user:ro` / `user:rw` / `user:deny`）。
    #[serde(default)]
    pub auth_users: String,
    /// secrets 文件路径（`user:password` 每行一条 ✗ 与 auth_users 配套启用认证）。
    #[serde(default)]
    pub secrets_file: String,
}

fn rsync_default_enabled() -> bool {
    false
}

fn rsync_default_port() -> u16 {
    873
}

fn rsync_default_module() -> String {
    "files".to_string()
}

impl RsyncConfig {
    pub fn bind_address(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }
}

impl S3Config {
    pub fn bind_address(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }
}

impl WebdavConfig {
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn webdav_from_env(
    explicit: Option<bool>,
    auth_enabled: bool,
) -> Result<WebdavConfig, ConfigError> {
    let enabled = resolve_webdav_enabled(explicit, auth_enabled)?;
    let host = std::env::var("VFILES_WEBDAV_HOST")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "0.0.0.0".to_string());
    let port = std::env::var("VFILES_WEBDAV_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(18080);
    // 共端口双轨（r-new ✓ 纯函数判定 ✗ 单测三式）
    let (embedded, mount_path) = resolve_webdav_dual(
        std::env::var("VFILES_WEBDAV_PORT").ok(),
        std::env::var("VFILES_WEBDAV_MOUNT").ok(),
    );
    Ok(WebdavConfig {
        enabled,
        host,
        port,
        embedded,
        mount_path,
    })
}

/// WebDAV 开关解析（r109b ✓ 对称 resolve_ftp_enabled 六分支 + **语义差**：
/// `(None, false)` = FTP 软停 ✗ WebDAV = **Err**（用户令「默认开启」+ auth 强制 =
/// 双保 ✓ 关 auth 必须显式关 WebDAV（VFILES_WEBDAV_ENABLED=false））。
/// S3 env 读取（对称式 ✗ 四变量：enabled/port/access/secret ✗ 空键 = runtime 随机判定）。
fn s3_from_env() -> Result<S3Config, ConfigError> {
    let enabled = std::env::var("VFILES_S3_ENABLED")
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let port = std::env::var("VFILES_S3_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9000);
    let access_key = std::env::var("VFILES_S3_ACCESS_KEY").unwrap_or_default();
    let secret_key = std::env::var("VFILES_S3_SECRET_KEY").unwrap_or_default();
    let credentials = std::env::var("VFILES_S3_CREDENTIALS")
        .ok()
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    Ok(S3Config {
        enabled,
        port,
        access_key,
        secret_key,
        credentials,
    })
}

/// rsync env 读取（对称式 ✗ 三变量）。
fn rsync_from_env() -> Result<RsyncConfig, ConfigError> {
    let enabled = std::env::var("VFILES_RSYNC_ENABLED")
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let port = std::env::var("VFILES_RSYNC_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(873);
    let module = std::env::var("VFILES_RSYNC_MODULE")
        .ok()
        .map(|m| m.trim_matches('/').to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "files".to_string());
    let writable = std::env::var("VFILES_RSYNC_WRITABLE")
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let auth_users = std::env::var("VFILES_RSYNC_AUTH_USERS")
        .ok()
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    let secrets_file = std::env::var("VFILES_RSYNC_SECRETS_FILE")
        .ok()
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    Ok(RsyncConfig {
        enabled,
        port,
        module,
        writable,
        auth_users,
        secrets_file,
    })
}

fn resolve_webdav_enabled(explicit: Option<bool>, auth_enabled: bool) -> Result<bool, ConfigError> {
    match (explicit, auth_enabled) {
        (Some(false), _) => Ok(false),
        (Some(true), false) => Err(ConfigError::LoadError(
            "显式开启 WebDAV（VFILES_WEBDAV_ENABLED=true）需要同时开启认证（VFILES_AUTH_ENABLED=true）：未认证的 WebDAV 允许任何人读写存储".to_string(),
        )),
        (Some(true), true) => Ok(true),
        (None, true) => Ok(true),
        (None, false) => Err(ConfigError::LoadError(
            "WebDAV 默认开启（用户令 ✓）需要认证：请开启 VFILES_AUTH_ENABLED=true，或显式关闭 WebDAV（VFILES_WEBDAV_ENABLED=false）".to_string(),
        )),
    }
}

/// 共端口双轨判定（r-new ✓ 纯函数单测）：显式 PORT = 独立模式（现行为回退）/
/// 未设 = 嵌入 ✗ mount 空串归一 `/dav`（**根挂载不支持 = 与前缀隔离方案核心一致** ✗
/// 空前缀会撞前端 fallback 根语义 → 归一防御）。
fn resolve_webdav_dual(port_explicit: Option<String>, mount: Option<String>) -> (bool, String) {
    let embedded = port_explicit.is_none();
    let mount_path = match mount.map(|m| m.trim().to_string()) {
        Some(m) if !m.is_empty() => {
            if m.starts_with('/') {
                m
            } else {
                format!("/{m}")
            }
        }
        _ => "/dav".to_string(),
    };
    (embedded, mount_path)
}

impl FtpConfig {
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    pub max_upload_size_bytes: u64,
    pub max_file_size_bytes: u64,
    pub upload_chunk_size_bytes: u64,
    pub rate_limit_requests_per_minute: u32,
    /// 缩略图磁盘缓存的条目上限；超出后按 mtime 回收最旧条目。
    pub thumbnail_cache_max_entries: usize,
    /// 缩略图磁盘缓存的字节上限；超出后同样触发回收。
    pub thumbnail_cache_max_bytes: u64,
    /// 是否允许输出 AVIF 缩略图（默认关闭）。
    ///
    /// AVIF 体积明显更小，但纯 Rust 编码器开销远高于 JPEG（实测 384px 缩略图
    /// 约 2.0s vs 0.004s），因此默认关闭，由运维按 CPU 余量决定是否开启。
    pub thumbnail_avif: bool,
}

/// 周期性维护（快照裁剪 + 孤儿 blob 回收）。
///
/// 默认关闭：这些操作会不可逆地删除数据，需运维显式开启。
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    pub enabled: bool,
    /// 两次维护之间的间隔（秒），最小 60 秒。
    pub interval_seconds: u64,
    /// 首次执行前的等待时间（秒），避免与启动/重启叠加。
    pub initial_delay_seconds: u64,
    /// 孤儿 blob 的保护期（秒）：创建时间晚于该窗口的 blob 不会被回收。
    pub blob_grace_seconds: u64,
    /// 每个命名空间保留的最新快照数；0 表示不裁剪快照。
    pub snapshot_keep: u32,
    /// 快照最长保留天数（0 表示不按时间裁剪，只按数量）。
    pub snapshot_max_age_days: u32,
}

/// 维护间隔的下限，避免配置成极小值后变成忙循环。
pub const MIN_MAINTENANCE_INTERVAL_SECONDS: u64 = 60;
/// 维护间隔的下限对应值：首次执行的等待时间上限。
pub const MAX_MAINTENANCE_INITIAL_DELAY_SECONDS: u64 = 300;

impl MaintenanceConfig {
    /// 首次执行前的等待时间：不超过间隔本身，也不超过 5 分钟。
    pub fn effective_initial_delay_seconds(&self) -> u64 {
        self.initial_delay_seconds
            .min(self.interval_seconds)
            .min(MAX_MAINTENANCE_INITIAL_DELAY_SECONDS)
    }
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub storage_root: Utf8PathBuf,
    pub database: Utf8PathBuf,
    pub blobs: Utf8PathBuf,
    pub uploads: Utf8PathBuf,
    pub tmp: Utf8PathBuf,
    pub export: Utf8PathBuf,
    pub logs: Utf8PathBuf,
    pub backups: Utf8PathBuf,
}

impl AppPaths {
    pub fn from_config(config: &StorageConfig) -> Self {
        let root = &config.root;
        Self {
            storage_root: root.clone(),
            database: config.database_path.clone(),
            blobs: root.join("blobs"),
            uploads: root.join("uploads"),
            tmp: root.join("tmp"),
            export: root.join("export"),
            logs: root.join("logs"),
            backups: root.join("backups"),
        }
    }
}

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load() -> Result<AppConfig, ConfigError> {
        // Get the current working directory and compute absolute paths
        let cwd = std::env::current_dir().map_err(|e| {
            ConfigError::LoadError(format!("Failed to get current directory: {}", e))
        })?;

        let cwd_utf8 = camino::Utf8PathBuf::from_path_buf(cwd).map_err(|_| {
            ConfigError::LoadError("Current directory is not valid UTF-8".to_string())
        })?;

        Self::load_from_dir(&cwd_utf8)
    }

    fn load_from_dir(cwd_utf8: &camino::Utf8Path) -> Result<AppConfig, ConfigError> {
        let runtime_root =
            Self::find_project_root(cwd_utf8).unwrap_or_else(|| cwd_utf8.to_path_buf());

        let data_dir = runtime_root.join("data");

        let host = Self::env_string(&["VFILES_HTTP_HOST"]).unwrap_or_else(|| "0.0.0.0".to_string());
        let port = Self::env_parse::<u16>(&["VFILES_HTTP_PORT"])?.unwrap_or(3000);
        let public_base_url = Self::env_url(&["VFILES_HTTP_PUBLIC_BASE_URL", "PUBLIC_BASE_URL"])?
            .unwrap_or_else(|| {
                Url::parse(&format!("http://localhost:{}", port))
                    .expect("default public base url should parse")
            });
        let cookie_secret = Self::env_string(&["VFILES_AUTH_COOKIE_SECRET", "AUTH_SECRET"])
            .map(|value| SecretString::new(value.into()))
            .unwrap_or_else(|| {
                SecretString::new(
                    "default-secret-key-for-dev-at-least-32-chars-long"
                        .to_string()
                        .into(),
                )
            });
        let cookie_secure_override =
            Self::env_parse_bool(&["VFILES_HTTP_COOKIE_SECURE", "HTTP_COOKIE_SECURE"])?;
        let (cors_allowed_origins, cors_allow_any_origin) =
            Self::env_cors_origins(&["VFILES_HTTP_CORS_ALLOWED_ORIGINS", "CORS_ORIGIN"])?
                .unwrap_or((Vec::new(), false));
        let storage_root = Self::env_string(&["VFILES_STORAGE_ROOT"])
            .map(Utf8PathBuf::from)
            .unwrap_or_else(|| data_dir.clone());
        let database_path = Self::env_string(&["VFILES_DATABASE_PATH"])
            .map(Utf8PathBuf::from)
            .unwrap_or_else(|| storage_root.join("vfiles.db"));
        let auth_enabled =
            Self::env_parse_bool(&["VFILES_AUTH_ENABLED", "ENABLE_AUTH"])?.unwrap_or(true);
        let allow_register =
            Self::env_parse_bool(&["VFILES_AUTH_ALLOW_REGISTER", "AUTH_ALLOW_REGISTER"])?
                .unwrap_or(true);
        let login_rate_limit_enabled = Self::env_parse_bool(&[
            "VFILES_AUTH_LOGIN_RATE_LIMIT_ENABLED",
            "AUTH_LOGIN_RATE_LIMIT_ENABLED",
        ])?
        .unwrap_or(true);
        let login_rate_limit_window_ms = Self::env_parse::<u64>(&[
            "VFILES_AUTH_LOGIN_RATE_LIMIT_WINDOW_MS",
            "AUTH_LOGIN_RATE_LIMIT_WINDOW_MS",
        ])?
        .unwrap_or(5 * 60 * 1000);
        let login_rate_limit_max_attempts = Self::env_parse::<u32>(&[
            "VFILES_AUTH_LOGIN_RATE_LIMIT_MAX",
            "AUTH_LOGIN_RATE_LIMIT_MAX",
        ])?
        .unwrap_or(10);

        // 全文（内容）搜索默认关闭：它需要读取并扫描文件内容，代价明显高于文件名搜索。
        let search_content_enabled =
            Self::env_parse_bool(&["VFILES_FEATURES_SEARCH_CONTENT", "FEATURES_SEARCH_CONTENT"])?
                .unwrap_or(false);
        let maintenance_snapshot_max_age_days = Self::env_parse::<u32>(&[
            "VFILES_MAINTENANCE_SNAPSHOT_MAX_AGE_DAYS",
            "MAINTENANCE_SNAPSHOT_MAX_AGE_DAYS",
        ])?
        .unwrap_or(0);
        let maintenance_enabled =
            Self::env_parse_bool(&["VFILES_MAINTENANCE_ENABLED", "MAINTENANCE_ENABLED"])?
                .unwrap_or(false);
        let maintenance_interval_seconds = Self::env_parse::<u64>(&[
            "VFILES_MAINTENANCE_INTERVAL_SECONDS",
            "MAINTENANCE_INTERVAL_SECONDS",
        ])?
        .unwrap_or(86_400)
        .max(MIN_MAINTENANCE_INTERVAL_SECONDS);
        let maintenance_initial_delay_seconds = Self::env_parse::<u64>(&[
            "VFILES_MAINTENANCE_INITIAL_DELAY_SECONDS",
            "MAINTENANCE_INITIAL_DELAY_SECONDS",
        ])?
        .unwrap_or(MAX_MAINTENANCE_INITIAL_DELAY_SECONDS);
        let maintenance_blob_grace_seconds = Self::env_parse::<u64>(&[
            "VFILES_MAINTENANCE_BLOB_GRACE_SECONDS",
            "MAINTENANCE_BLOB_GRACE_SECONDS",
        ])?
        .unwrap_or(3600);
        // 0 表示不裁剪快照；默认不裁剪，避免静默丢历史
        let maintenance_snapshot_keep = Self::env_parse::<u32>(&[
            "VFILES_MAINTENANCE_SNAPSHOT_KEEP",
            "MAINTENANCE_SNAPSHOT_KEEP",
        ])?
        .unwrap_or(0);

        let thumbnail_cache_max_entries = Self::env_parse::<usize>(&[
            "VFILES_THUMBNAIL_CACHE_MAX_ENTRIES",
            "THUMBNAIL_CACHE_MAX_ENTRIES",
        ])?
        .unwrap_or(2000)
        .max(1);
        // 以 MB 配置更符合运维直觉；0 表示不退让上限（由条目数兜底）
        let thumbnail_cache_max_mb =
            Self::env_parse::<u64>(&["VFILES_THUMBNAIL_CACHE_MAX_MB", "THUMBNAIL_CACHE_MAX_MB"])?
                .unwrap_or(256);
        let thumbnail_cache_max_bytes = thumbnail_cache_max_mb.saturating_mul(1024 * 1024);
        let thumbnail_avif =
            Self::env_parse_bool(&["VFILES_THUMBNAIL_AVIF", "THUMBNAIL_AVIF"])?.unwrap_or(false);

        // FTP 默认开启（客户端批量导入最常用的通道）；认证关闭时无法安全提供 FTP，
        // 因此下面的 ftp_enabled 计算会把「默认开启」在无认证场景下降级为关闭。
        let ftp_enabled_raw = Self::env_parse_bool(&["VFILES_FTP_ENABLED", "FTP_ENABLED"])?;
        let webdav_enabled_raw =
            Self::env_parse_bool(&["VFILES_WEBDAV_ENABLED", "WEBDAV_ENABLED"])?;
        let ftp_enabled = Self::resolve_ftp_enabled(ftp_enabled_raw, auth_enabled)?;

        let ftp = FtpConfig {
            enabled: ftp_enabled,
            host: Self::env_string(&["VFILES_FTP_HOST"]).unwrap_or_else(|| "0.0.0.0".to_string()),
            port: Self::env_parse::<u16>(&["VFILES_FTP_PORT"])?.unwrap_or(2121),
            passive_ports: {
                let raw = Self::env_string(&["VFILES_FTP_PASSIVE_PORTS"])
                    .unwrap_or_else(|| "50000-50100".to_string());
                Self::parse_port_range(&raw)?
            },
            passive_host: Self::env_string(&["VFILES_FTP_PASSIVE_HOST"]),
            max_connections: Self::env_parse::<u32>(&["VFILES_FTP_MAX_CONNECTIONS"])?
                .unwrap_or(8)
                .max(1),
            idle_timeout_seconds: Self::env_parse::<u64>(&["VFILES_FTP_IDLE_TIMEOUT_SECONDS"])?
                .unwrap_or(300)
                .max(30),
            allowed_roles: Self::env_string(&["VFILES_FTP_ALLOWED_ROLES"])
                .unwrap_or_else(|| "admin,manager".to_string())
                .split(',')
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
                .collect(),
            tls_cert: Self::env_string(&["VFILES_FTP_TLS_CERT"]),
            tls_key: Self::env_string(&["VFILES_FTP_TLS_KEY"]),
            tls_required: Self::env_parse_bool(&["VFILES_FTP_TLS_REQUIRED"])?.unwrap_or(false),
            snapshot_mode: Self::env_string(&["VFILES_FTP_SNAPSHOT_MODE"])
                .unwrap_or_else(|| "batch".to_string())
                .to_ascii_lowercase(),
            snapshot_flush_files: Self::env_parse::<u32>(&["VFILES_FTP_SNAPSHOT_FLUSH_FILES"])?
                .unwrap_or(200)
                .max(1),
        };

        let ftp_enabled = ftp.enabled;
        // 单文件上限同时用于特性矩阵（前端提示与预校验）与请求限制
        let max_file_size_bytes = Self::resolve_max_file_size_bytes(Self::env_parse::<u64>(&[
            "VFILES_MAX_FILE_SIZE_MB",
        ])?);
        let config = AppConfig {
            http: HttpConfig {
                host,
                port,
                public_base_url,
                cookie_secret,
                cookie_secure_override,
                cors_allowed_origins,
                cors_allow_any_origin,
            },
            storage: StorageConfig {
                root: storage_root,
                database_path,
            },
            auth: AuthConfig {
                enabled: auth_enabled,
                allow_register,
                session_ttl_seconds: 86400,
                login_rate_limit: LoginRateLimitConfig {
                    enabled: login_rate_limit_enabled,
                    window_ms: login_rate_limit_window_ms,
                    max_attempts: login_rate_limit_max_attempts,
                },
                password_reset_ttl_seconds: 3600,
                email_login_code_ttl_seconds: 600,
            },
            mail: MailConfig {
                smtp_host: None,
                smtp_port: None,
                smtp_username: None,
                smtp_password: None,
                from_address: None,
            },
            search: SearchConfig {
                enabled: false,
                max_results: 100,
            },
            limits: LimitsConfig {
                max_upload_size_bytes: 4_u64 * 1024 * 1024 * 1024, // 4096MB
                max_file_size_bytes,
                upload_chunk_size_bytes: 5 * 1024 * 1024, // 5MB
                rate_limit_requests_per_minute: 60,
                thumbnail_cache_max_entries,
                thumbnail_cache_max_bytes,
                thumbnail_avif,
            },
            maintenance: MaintenanceConfig {
                enabled: maintenance_enabled,
                interval_seconds: maintenance_interval_seconds,
                initial_delay_seconds: maintenance_initial_delay_seconds,
                blob_grace_seconds: maintenance_blob_grace_seconds,
                snapshot_keep: maintenance_snapshot_keep,
                snapshot_max_age_days: maintenance_snapshot_max_age_days,
            },
            ftp,
            webdav: webdav_from_env(webdav_enabled_raw, auth_enabled)?,
            s3: s3_from_env()?,
            rsync: rsync_from_env()?,
            features: FeatureMatrix {
                auth_enabled,
                multi_user: true,
                email_login: false,
                search_content: search_content_enabled,
                share_enabled: true,
                history_enabled: true,
                ftp_enabled,
                max_file_size_bytes,
            },
        };
        Self::validate(&config)?;
        Ok(config)
    }

    /// 单文件上限：`VFILES_MAX_FILE_SIZE_MB`（MB）覆盖默认 4096MB，至少 1MB。
    fn resolve_max_file_size_bytes(env_mb: Option<u64>) -> u64 {
        const DEFAULT_MB: u64 = 4096;
        let mb = env_mb.unwrap_or(DEFAULT_MB).max(1);
        mb.saturating_mul(1024 * 1024)
    }

    /// 解析 FTP 开关：默认开启，但认证关闭时无法安全提供 FTP。
    ///
    /// - 显式 `true` + 认证关闭 ⇒ 报错（用户明确要求，必须说明原因）；
    /// - 未显式设置 + 认证关闭 ⇒ 自动停用并告警（不阻塞 `serve` 启动）；
    /// - 其余情况按显式值处理，未设置时默认开启。
    fn resolve_ftp_enabled(
        explicit: Option<bool>,
        auth_enabled: bool,
    ) -> Result<bool, ConfigError> {
        match (explicit, auth_enabled) {
            (Some(false), _) => Ok(false),
            (Some(true), false) => Err(ConfigError::LoadError(
                "显式开启 FTP（VFILES_FTP_ENABLED=true）需要同时开启认证（VFILES_AUTH_ENABLED=true）：未认证的 FTP 允许任何人写入存储"
                    .to_string(),
            )),
            (Some(true), true) => Ok(true),
            (None, true) => Ok(true),
            (None, false) => {
                tracing::warn!(
                    "认证已关闭（VFILES_AUTH_ENABLED=false），FTP 批量导入自动停用；如需使用请先开启认证"
                );
                Ok(false)
            }
        }
    }

    /// 解析 `50000-50100` 形式的端口段。
    fn parse_port_range(raw: &str) -> Result<(u16, u16), ConfigError> {
        let (start, end) = raw.split_once('-').ok_or_else(|| {
            ConfigError::LoadError(format!(
                "VFILES_FTP_PASSIVE_PORTS 需要 `起始-结束` 形式，实际: {raw}"
            ))
        })?;
        let start = start.trim().parse::<u16>().map_err(|err| {
            ConfigError::LoadError(format!("VFILES_FTP_PASSIVE_PORTS 起始端口非法: {err}"))
        })?;
        let end = end.trim().parse::<u16>().map_err(|err| {
            ConfigError::LoadError(format!("VFILES_FTP_PASSIVE_PORTS 结束端口非法: {err}"))
        })?;
        if start == 0 || end == 0 || end < start {
            return Err(ConfigError::LoadError(format!(
                "VFILES_FTP_PASSIVE_PORTS 区间非法: {raw}"
            )));
        }
        Ok((start, end))
    }

    pub fn validate(config: &AppConfig) -> Result<(), ConfigError> {
        if config.http.cookie_secret.expose_secret().len() < 32 {
            return Err(ConfigError::InvalidCookieSecret);
        }
        if config.storage.root.as_str().is_empty() {
            return Err(ConfigError::InvalidStorageRoot);
        }

        if config.ftp.enabled {
            if config.ftp.port == 0 {
                return Err(ConfigError::LoadError(
                    "VFILES_FTP_PORT 不能为 0".to_string(),
                ));
            }
            if config.ftp.port == config.http.port {
                return Err(ConfigError::LoadError(
                    "VFILES_FTP_PORT 不能与 VFILES_HTTP_PORT 相同".to_string(),
                ));
            }
            if (config.ftp.tls_cert.is_some()) != (config.ftp.tls_key.is_some()) {
                return Err(ConfigError::LoadError(
                    "FTPS 需要同时配置 VFILES_FTP_TLS_CERT 与 VFILES_FTP_TLS_KEY".to_string(),
                ));
            }
            if config.ftp.tls_required && config.ftp.tls_cert.is_none() {
                return Err(ConfigError::LoadError(
                    "VFILES_FTP_TLS_REQUIRED=true 需要配置证书与私钥".to_string(),
                ));
            }
            if !matches!(
                config.ftp.snapshot_mode.as_str(),
                "batch" | "per-file" | "off"
            ) {
                return Err(ConfigError::LoadError(format!(
                    "VFILES_FTP_SNAPSHOT_MODE 只能是 batch/per-file/off，实际: {}",
                    config.ftp.snapshot_mode
                )));
            }
            // 防御性检查：加载阶段已保证「认证关闭 ⇒ FTP 关闭」，这里防止后续改动绕过
            if !config.auth.enabled {
                return Err(ConfigError::LoadError(
                    "启用 FTP 需要开启认证（VFILES_AUTH_ENABLED=true）".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn find_project_root(start_dir: &camino::Utf8Path) -> Option<camino::Utf8PathBuf> {
        let mut current = start_dir.to_path_buf();
        loop {
            if Path::new(&current).join("Cargo.toml").exists() {
                return Some(current);
            }
            if !current.pop() {
                break;
            }
        }
        None
    }

    pub fn resolve_paths(config: &AppConfig) -> AppPaths {
        AppPaths::from_config(&config.storage)
    }

    pub fn effective_public_base_url(config: &AppConfig) -> &Url {
        &config.http.public_base_url
    }

    fn env_string(keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|key| {
            env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
    }

    fn env_parse<T>(keys: &[&str]) -> Result<Option<T>, ConfigError>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        Self::env_string(keys)
            .map(|raw| {
                raw.parse::<T>().map_err(|err| {
                    ConfigError::LoadError(format!("Invalid value for {}: {}", keys.join("/"), err))
                })
            })
            .transpose()
    }

    fn env_parse_bool(keys: &[&str]) -> Result<Option<bool>, ConfigError> {
        Self::env_string(keys)
            .map(|raw| match raw.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Ok(true),
                "0" | "false" | "no" | "off" => Ok(false),
                _ => Err(ConfigError::LoadError(format!(
                    "Invalid boolean value for {}: {}",
                    keys.join("/"),
                    raw
                ))),
            })
            .transpose()
    }

    fn env_url(keys: &[&str]) -> Result<Option<Url>, ConfigError> {
        Self::env_string(keys)
            .map(|raw| {
                Url::parse(&raw).map_err(|err| {
                    ConfigError::LoadError(format!(
                        "Invalid URL value for {}: {}",
                        keys.join("/"),
                        err
                    ))
                })
            })
            .transpose()
    }

    fn env_cors_origins(keys: &[&str]) -> Result<Option<(Vec<String>, bool)>, ConfigError> {
        let Some(raw) = Self::env_string(keys) else {
            return Ok(None);
        };

        let mut allow_any = false;
        let mut origins = Vec::new();

        for value in raw
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if value == "*" {
                allow_any = true;
                origins.clear();
                break;
            }

            let origin = Url::parse(value).map_err(|err| {
                ConfigError::LoadError(format!(
                    "Invalid CORS origin for {}: {}",
                    keys.join("/"),
                    err
                ))
            })?;
            origins.push(origin.origin().ascii_serialization());
        }

        Ok(Some((origins, allow_any)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_config_load() {
        let config = ConfigLoader::load().unwrap();
        assert_eq!(config.http.port, 3000);
        // r2: S3 新面默认关 + 专用端口 9000（显式 VFILES_S3_ENABLED 启用）
        assert!(!config.s3.enabled, "S3 默认关（新协议面显式启用原则）");
        assert_eq!(config.s3.port, 9000);
        // r3: rsync 新面默认关 + 873 + 模块 files（显式 VFILES_RSYNC_ENABLED 启用）
        assert!(
            !config.rsync.enabled,
            "rsync 默认关（新协议面显式启用原则）"
        );
        assert_eq!(config.rsync.port, 873);
        assert_eq!(config.rsync.module, "files");
        assert!(config.auth.enabled);
        assert!(config.auth.login_rate_limit.enabled);
        assert_eq!(config.auth.login_rate_limit.window_ms, 300_000);
        assert_eq!(config.auth.login_rate_limit.max_attempts, 10);
    }

    #[test]
    fn test_ftp_defaults_are_enabled_and_sane() {
        let config = ConfigLoader::load().unwrap();

        assert!(
            config.ftp.enabled,
            "FTP 默认开启（认证开启时），以便直接批量导入"
        );
        assert_eq!(config.ftp.port, 2121);
        assert_eq!(config.ftp.passive_ports, (50_000, 50_100));
        assert_eq!(config.ftp.allowed_roles, vec!["admin", "manager"]);
        assert_eq!(config.ftp.snapshot_mode, "batch");
        assert_eq!(config.ftp.snapshot_flush_files, 200);
        assert!(!config.ftp.tls_required);
        assert!(config.ftp.tls_cert.is_none());
    }

    #[test]
    fn test_ftp_passive_range_parsing() {
        assert_eq!(
            ConfigLoader::parse_port_range("50000-50100").unwrap(),
            (50_000, 50_100)
        );
        assert_eq!(
            ConfigLoader::parse_port_range(" 21000 - 21010 ").unwrap(),
            (21_000, 21_010)
        );
        assert_eq!(
            ConfigLoader::parse_port_range("2121-2121").unwrap(),
            (2121, 2121)
        );

        assert!(ConfigLoader::parse_port_range("50000").is_err(), "缺少区间");
        assert!(
            ConfigLoader::parse_port_range("50100-50000").is_err(),
            "起止倒置"
        );
        assert!(ConfigLoader::parse_port_range("0-100").is_err(), "0 非法");
        assert!(ConfigLoader::parse_port_range("a-b").is_err(), "非数字");
    }

    /// 开关解析是纯函数，避免测试之间通过环境变量互相干扰。
    #[test]
    fn test_ftp_switch_resolution() {
        // 默认（认证开启）⇒ 开启；显式关闭 ⇒ 关闭
        assert!(ConfigLoader::resolve_ftp_enabled(None, true).unwrap());
        assert!(!ConfigLoader::resolve_ftp_enabled(Some(false), true).unwrap());
        assert!(!ConfigLoader::resolve_ftp_enabled(Some(false), false).unwrap());
        // 显式开启 + 认证开启 ⇒ 开启
        assert!(ConfigLoader::resolve_ftp_enabled(Some(true), true).unwrap());
        // 认证关闭 + 未显式设置 ⇒ 自动停用（不阻塞 serve）
        assert!(!ConfigLoader::resolve_ftp_enabled(None, false).unwrap());
        // 认证关闭 + 显式开启 ⇒ 报错并说明原因
        let err = ConfigLoader::resolve_ftp_enabled(Some(true), false)
            .expect_err("explicit FTP with auth off must fail");
        assert!(
            err.to_string().contains("需要同时开启认证"),
            "错误信息应说明原因，实际: {err}"
        );
    }

    #[test]
    fn test_ftp_validation_rules() {
        fn config_with_ftp(ftp: FtpConfig, http_port: u16, auth_enabled: bool) -> AppConfig {
            let mut config = ConfigLoader::load().unwrap();
            config.http.port = http_port;
            config.auth.enabled = auth_enabled;
            config.ftp = ftp;
            config
        }

        let mut ftp = ConfigLoader::load().unwrap().ftp;
        ftp.enabled = true;
        // 端口与 HTTP 冲突
        assert!(ConfigLoader::validate(&config_with_ftp(ftp.clone(), 2121, true)).is_err());
        // 未开启认证
        assert!(ConfigLoader::validate(&config_with_ftp(ftp.clone(), 3000, false)).is_err());
        // 只配置证书不配置私钥
        let mut half_tls = ftp.clone();
        half_tls.tls_cert = Some("/etc/vfiles/cert.pem".to_string());
        assert!(ConfigLoader::validate(&config_with_ftp(half_tls, 3000, true)).is_err());
        // 要求 FTPS 但没有证书
        let mut required = ftp.clone();
        required.tls_required = true;
        assert!(ConfigLoader::validate(&config_with_ftp(required, 3000, true)).is_err());
        // 非法快照策略
        let mut bad_mode = ftp.clone();
        bad_mode.snapshot_mode = "sometimes".to_string();
        assert!(ConfigLoader::validate(&config_with_ftp(bad_mode, 3000, true)).is_err());
        // 合法配置
        assert!(ConfigLoader::validate(&config_with_ftp(ftp, 3000, true)).is_ok());
    }

    #[test]
    fn test_thumbnail_cache_limits_have_sane_defaults() {
        let config = ConfigLoader::load().unwrap();

        assert_eq!(config.limits.thumbnail_cache_max_entries, 2000);
        assert_eq!(config.limits.thumbnail_cache_max_bytes, 256 * 1024 * 1024);
    }

    #[test]
    fn test_maintenance_defaults_are_conservative() {
        let config = ConfigLoader::load().unwrap();

        assert!(
            !config.maintenance.enabled,
            "周期性维护涉及不可逆删除，必须显式开启"
        );
        assert_eq!(config.maintenance.interval_seconds, 86_400);
        assert_eq!(config.maintenance.initial_delay_seconds, 300);
        assert_eq!(config.maintenance.blob_grace_seconds, 3_600);
        assert_eq!(
            config.maintenance.snapshot_keep, 0,
            "默认不裁剪快照，避免静默丢失历史"
        );
    }

    #[test]
    fn test_max_file_size_limit_resolution() {
        assert_eq!(
            ConfigLoader::resolve_max_file_size_bytes(None),
            4096 * 1024 * 1024
        );
        assert_eq!(
            ConfigLoader::resolve_max_file_size_bytes(Some(64)),
            64 * 1024 * 1024
        );
        // 0 或异常小的值按 1MB 兜底，避免把服务锁死
        assert_eq!(
            ConfigLoader::resolve_max_file_size_bytes(Some(0)),
            1024 * 1024
        );
    }

    #[test]
    fn test_maintenance_initial_delay_is_bounded() {
        let mut config = MaintenanceConfig {
            enabled: true,
            interval_seconds: 3_600,
            initial_delay_seconds: 10_000,
            blob_grace_seconds: 3_600,
            snapshot_keep: 10,
            snapshot_max_age_days: 0,
        };
        assert_eq!(
            config.effective_initial_delay_seconds(),
            MAX_MAINTENANCE_INITIAL_DELAY_SECONDS
        );

        // 首次等待不得超过间隔本身
        config.interval_seconds = 120;
        assert_eq!(config.effective_initial_delay_seconds(), 120);

        config.initial_delay_seconds = 5;
        assert_eq!(config.effective_initial_delay_seconds(), 5);
    }

    #[test]
    fn test_validate_cookie_secret() {
        let mut config = ConfigLoader::load().unwrap();
        config.http.cookie_secret = SecretString::new("short".to_string().into());
        assert!(ConfigLoader::validate(&config).is_err());
    }

    #[test]
    fn test_https_public_base_url_enables_secure_cookie() {
        let mut config = ConfigLoader::load().unwrap();
        config.http.public_base_url = Url::parse("https://files.example.test").unwrap();
        assert!(config.http.cookie_secure());
    }

    #[test]
    fn test_cookie_secure_override_can_disable_secure_cookie_for_https_public_base_url() {
        let mut config = ConfigLoader::load().unwrap();
        config.http.public_base_url = Url::parse("https://files.example.test").unwrap();
        config.http.cookie_secure_override = Some(false);

        assert!(!config.http.cookie_secure());
    }

    #[test]
    fn test_cookie_secure_override_can_enable_secure_cookie_for_http_public_base_url() {
        let mut config = ConfigLoader::load().unwrap();
        config.http.public_base_url = Url::parse("http://192.168.1.10:3000").unwrap();
        config.http.cookie_secure_override = Some(true);

        assert!(config.http.cookie_secure());
    }

    #[test]
    fn test_cors_defaults_to_public_base_origin() {
        let mut config = ConfigLoader::load().unwrap();
        config.http.public_base_url = Url::parse("https://files.example.test:8443/app").unwrap();
        config.http.cors_allowed_origins.clear();
        config.http.cors_allow_any_origin = false;

        assert_eq!(
            config.http.effective_cors_allowed_origins(),
            vec!["https://files.example.test:8443".to_string()]
        );
    }

    #[test]
    fn test_app_paths() {
        let storage = StorageConfig {
            root: Utf8PathBuf::from("/tmp/storage"),
            database_path: Utf8PathBuf::from("/tmp/vfiles.db"),
        };
        let paths = AppPaths::from_config(&storage);
        assert_eq!(paths.blobs, Utf8PathBuf::from("/tmp/storage/blobs"));
    }

    #[test]
    fn test_load_from_dir_without_project_root_uses_working_directory_defaults() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "vfiles-config-no-project-root-{}-{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let cwd_utf8 = Utf8PathBuf::from_path_buf(temp_dir.clone())
            .expect("temp dir path should be valid utf-8");
        let config = ConfigLoader::load_from_dir(&cwd_utf8)
            .expect("config should load outside the source tree");

        assert_eq!(config.storage.root, cwd_utf8.join("data"));
        assert_eq!(
            config.storage.database_path,
            cwd_utf8.join("data").join("vfiles.db")
        );

        std::fs::remove_dir_all(&temp_dir).expect("temp dir should be removed");
    }
}

#[cfg(test)]
mod webdav_enabled_semantics {
    use super::resolve_webdav_enabled;

    #[test]
    fn default_on_requires_auth_per_user_directive() {
        // (None, false) = **Err**（用户令「默认开启」+ auth 强制 = 双保 ✓
        // 语义差 = FTP `(None,false)→false` 软停 ✗ WebDAV 严格 ✓）
        assert!(resolve_webdav_enabled(None, false).is_err());
        assert!(resolve_webdav_enabled(None, true).is_ok());
    }

    #[test]
    fn explicit_off_and_explicit_on_forms() {
        assert_eq!(resolve_webdav_enabled(Some(false), false).unwrap(), false);
        assert!(resolve_webdav_enabled(Some(true), false).is_err());
        assert!(resolve_webdav_enabled(Some(true), true).is_ok());
    }
}

#[cfg(test)]
mod webdav_dual_tests {
    use super::resolve_webdav_dual;

    #[test]
    fn dual_track_modes() {
        // r-new 三式守护：未设 PORT=嵌入 /dav · 显式 PORT=独立回退 · mount 覆写与归一
        assert_eq!(resolve_webdav_dual(None, None), (true, "/dav".to_string()));
        assert_eq!(
            resolve_webdav_dual(Some("18080".into()), None),
            (false, "/dav".to_string())
        );
        assert_eq!(
            resolve_webdav_dual(None, Some("/webdav".into())),
            (true, "/webdav".to_string())
        );
        // 空/无前导斜杠 → 归一防御（根挂载不支持 ✗ 前缀隔离是方案核心）
        assert_eq!(
            resolve_webdav_dual(None, Some("".into())),
            (true, "/dav".to_string())
        );
        assert_eq!(
            resolve_webdav_dual(None, Some("dav2".into())),
            (true, "/dav2".to_string())
        );
    }
}
