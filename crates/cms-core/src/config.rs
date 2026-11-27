use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub public_url: String,
    pub database_url: String,
    pub redis_url: Option<String>,
    pub jwt_secret: String,
    pub upload_dir: String,
    pub cors_origins: Vec<String>,
    pub log_format: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let host = env::var("RAPI_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("RAPI_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(1337);
        let public_url = env::var("RAPI_PUBLIC_URL")
            .unwrap_or_else(|_| format!("http://localhost:{port}"));
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://data/rapi.db?mode=rwc".to_string());
        let redis_url = env::var("REDIS_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let jwt_secret = env::var("JWT_SECRET")
            .unwrap_or_else(|_| "dev-insecure-jwt-secret-replace-in-production".to_string());
        let upload_dir = env::var("UPLOAD_DIR").unwrap_or_else(|_| "./data/uploads".to_string());
        let cors_origins = env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:5173,http://127.0.0.1:5173".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let log_format = env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string());

        Self {
            host,
            port,
            public_url,
            database_url,
            redis_url,
            jwt_secret,
            upload_dir,
            cors_origins,
            log_format,
        }
    }

    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
