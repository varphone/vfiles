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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    pub max_upload_size_bytes: u64,
    pub max_file_size_bytes: u64,
    pub upload_chunk_size_bytes: u64,
    pub rate_limit_requests_per_minute: u32,
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
                max_file_size_bytes: 4_u64 * 1024 * 1024 * 1024,   // 4096MB
                upload_chunk_size_bytes: 5 * 1024 * 1024,          // 5MB
                rate_limit_requests_per_minute: 60,
            },
            features: FeatureMatrix {
                auth_enabled,
                multi_user: true,
                email_login: false,
                search_content: false,
                share_enabled: true,
                history_enabled: true,
            },
        };
        Self::validate(&config)?;
        Ok(config)
    }

    pub fn validate(config: &AppConfig) -> Result<(), ConfigError> {
        if config.http.cookie_secret.expose_secret().len() < 32 {
            return Err(ConfigError::InvalidCookieSecret);
        }
        if config.storage.root.as_str().is_empty() {
            return Err(ConfigError::InvalidStorageRoot);
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
        assert!(config.auth.enabled);
        assert!(config.auth.login_rate_limit.enabled);
        assert_eq!(config.auth.login_rate_limit.window_ms, 300_000);
        assert_eq!(config.auth.login_rate_limit.max_attempts, 10);
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
