use crate::error::{ApiError, ApiResult};
use rand::Rng;
use serde::Deserialize;
use std::env;
use std::fmt;
use std::path::Path;

const KNOWN_INSECURE_SECRETS: &[&str] = &[
    "dev-insecure-jwt-secret-replace-in-production",
    "change-me-in-production-use-at-least-32-random-bytes",
    "change-me-in-production-32-bytes-minimum",
    "secret",
    "changeme",
];

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    server: ServerSection,
    #[serde(default)]
    database: DatabaseSection,
    #[serde(default)]
    storage: StorageSection,
}

#[derive(Debug, Default, Deserialize)]
struct ServerSection {
    env: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    public_url: Option<String>,
    cors_origins: Option<Vec<String>>,
    log_format: Option<String>,
    enable_docs: Option<bool>,
    request_timeout_secs: Option<u64>,
    max_body_bytes: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct DatabaseSection {
    url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StorageSection {
    upload_dir: Option<String>,
    web_dist_dir: Option<String>,
}

#[derive(Clone)]
pub struct AppConfig {
    pub env: String,
    pub host: String,
    pub port: u16,
    pub public_url: String,
    pub database_url: String,
    pub redis_url: Option<String>,
    pub jwt_secret: String,
    pub upload_dir: String,
    pub web_dist_dir: String,
    pub cors_origins: Vec<String>,
    pub log_format: String,
    pub enable_docs: bool,
    pub request_timeout_secs: u64,
    pub max_body_bytes: usize,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("env", &self.env)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("public_url", &self.public_url)
            .field("database_url", &self.database_url)
            .field("redis_url", &self.redis_url.as_deref().map(redact_url_secret))
            .field("jwt_secret", &"[REDACTED]")
            .field("upload_dir", &self.upload_dir)
            .field("web_dist_dir", &self.web_dist_dir)
            .field("cors_origins", &self.cors_origins)
            .field("log_format", &self.log_format)
            .field("enable_docs", &self.enable_docs)
            .field("request_timeout_secs", &self.request_timeout_secs)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish()
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let file_cfg = load_default_toml("config/default.toml").unwrap_or_default();

        let env_name = env::var("RAPI_ENV")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or(file_cfg.server.env)
            .unwrap_or_else(|| "development".to_string());

        let is_prod = env_name.eq_ignore_ascii_case("production");

        let host = env::var("RAPI_HOST")
            .ok()
            .or(file_cfg.server.host)
            .unwrap_or_else(|| "0.0.0.0".to_string());

        let port = env::var("RAPI_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .or(file_cfg.server.port)
            .unwrap_or(1337);

        let public_url = env::var("RAPI_PUBLIC_URL")
            .ok()
            .or(file_cfg.server.public_url)
            .unwrap_or_else(|| format!("http://localhost:{port}"));

        let database_url = env::var("DATABASE_URL")
            .ok()
            .or(file_cfg.database.url)
            .unwrap_or_else(|| "sqlite://data/rapi.db?mode=rwc".to_string());

        let redis_url = env::var("REDIS_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let raw_jwt = env::var("JWT_SECRET")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // In development, generate a fresh ephemeral 32-byte random secret if none is provided
        // instead of using a static hardcoded secret. In production, leave empty so validate() fails fast.
        let jwt_secret = match raw_jwt {
            Some(secret) => secret,
            None if !is_prod => generate_ephemeral_hex_secret(),
            None => String::new(),
        };

        let upload_dir = env::var("UPLOAD_DIR")
            .ok()
            .or(file_cfg.storage.upload_dir)
            .unwrap_or_else(|| "./data/uploads".to_string());

        let web_dist_dir = env::var("WEB_DIST_DIR")
            .ok()
            .or(file_cfg.storage.web_dist_dir)
            .unwrap_or_else(|| "./web/dist".to_string());

        let cors_origins = env::var("CORS_ORIGINS")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .or(file_cfg.server.cors_origins)
            .unwrap_or_else(|| {
                vec![
                    "http://localhost:5173".to_string(),
                    "http://127.0.0.1:5173".to_string(),
                ]
            });

        let log_format = env::var("LOG_FORMAT")
            .ok()
            .or(file_cfg.server.log_format)
            .unwrap_or_else(|| if is_prod { "json" } else { "pretty" }.to_string());

        let enable_docs = env::var("ENABLE_DOCS")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .or(if is_prod { Some(false) } else { file_cfg.server.enable_docs })
            .unwrap_or(!is_prod);

        let request_timeout_secs = env::var("REQUEST_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .or(file_cfg.server.request_timeout_secs)
            .unwrap_or(30);

        let max_body_bytes = env::var("MAX_BODY_BYTES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .or(file_cfg.server.max_body_bytes)
            .unwrap_or(10 * 1024 * 1024);

        Self {
            env: env_name,
            host,
            port,
            public_url,
            database_url,
            redis_url,
            jwt_secret,
            upload_dir,
            web_dist_dir,
            cors_origins,
            log_format,
            enable_docs,
            request_timeout_secs,
            max_body_bytes,
        }
    }

    pub fn is_production(&self) -> bool {
        self.env.eq_ignore_ascii_case("production")
    }

    pub fn validate(&self) -> ApiResult<()> {
        if self.is_production() {
            if self.jwt_secret.len() < 32 {
                return Err(ApiError::Internal(
                    "JWT_SECRET must be set and at least 32 characters long in production"
                        .to_string(),
                ));
            }
            if KNOWN_INSECURE_SECRETS
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&self.jwt_secret))
            {
                return Err(ApiError::Internal(
                    "JWT_SECRET cannot use a known default placeholder in production".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn generate_ephemeral_hex_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn redact_url_secret(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let rest = &url[scheme_end + 3..];
        if let Some(at_pos) = rest.find('@') {
            return format!("{}://[REDACTED]@{}", &url[..scheme_end], &rest[at_pos + 1..]);
        }
    }
    url.to_string()
}

fn load_default_toml(path: &str) -> Option<FileConfig> {
    let p = Path::new(path);
    if !p.exists() {
        return None;
    }
    let content = std::fs::read_to_string(p).ok()?;
    toml::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_secrets() {
        let mut cfg = AppConfig::from_env();
        cfg.jwt_secret = "super-secret-value-12345678901234567890".to_string();
        cfg.redis_url = Some("redis://:my-redis-pass@127.0.0.1:6379".to_string());

        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("super-secret-value"));
        assert!(!dbg.contains("my-redis-pass"));
        assert!(dbg.contains("[REDACTED]"));
    }

    #[test]
    fn production_rejects_weak_or_placeholder_jwt_secret() {
        let mut cfg = AppConfig::from_env();
        cfg.env = "production".to_string();
        cfg.jwt_secret = "short".to_string();
        assert!(cfg.validate().is_err());

        cfg.jwt_secret = "change-me-in-production-32-bytes-minimum".to_string();
        assert!(cfg.validate().is_err());

        cfg.jwt_secret = "a".repeat(48);
        assert!(cfg.validate().is_ok());
    }
}
