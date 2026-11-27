use serde::Deserialize;
use std::env;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    server: ServerSection,
    #[serde(default)]
    database: DatabaseSection,
    #[serde(default)]
    storage: StorageSection,
    #[serde(default)]
    security: SecuritySection,
}

#[derive(Debug, Default, Deserialize)]
struct ServerSection {
    host: Option<String>,
    port: Option<u16>,
    public_url: Option<String>,
    cors_origins: Option<Vec<String>>,
    log_format: Option<String>,
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

#[derive(Debug, Default, Deserialize)]
struct SecuritySection {
    jwt_secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
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
}

impl AppConfig {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let file_cfg = load_default_toml("config/default.toml").unwrap_or_default();

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

        let jwt_secret = env::var("JWT_SECRET")
            .ok()
            .or(file_cfg.security.jwt_secret)
            .unwrap_or_else(|| "dev-insecure-jwt-secret-replace-in-production".to_string());

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
            .unwrap_or_else(|| "pretty".to_string());

        Self {
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
        }
    }

    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn load_default_toml(path: &str) -> Option<FileConfig> {
    let p = Path::new(path);
    if !p.exists() {
        return None;
    }
    let content = std::fs::read_to_string(p).ok()?;
    toml::from_str(&content).ok()
}
