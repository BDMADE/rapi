use cms_core::{ApiError, ApiResult};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use tracing::info;

/// Split SQLite connection pools:
/// - `writer`: single serialized connection (`max_connections = 1`) to eliminate writer contention.
/// - `reader`: concurrent read pool (`max_connections = 8`) leveraging WAL snapshot isolation.
#[derive(Debug, Clone)]
pub struct DbPools {
    pub writer: SqlitePool,
    pub reader: SqlitePool,
}

impl DbPools {
    pub async fn connect(database_url: &str) -> ApiResult<Self> {
        let is_memory = database_url.contains(":memory:") || database_url == "sqlite::memory:";

        if !is_memory {
            ensure_sqlite_parent_dir(database_url)?;
        }

        if is_memory {
            let opts = SqliteConnectOptions::from_str(database_url)
                .map_err(|e| ApiError::Internal(format!("Invalid DATABASE_URL: {e}")))?
                .foreign_keys(true)
                .busy_timeout(Duration::from_secs(5));

            // Single shared connection for `:memory:` so reader & writer share the in-memory DB.
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .map_err(|e| ApiError::Internal(format!("SQLite memory connect failed: {e}")))?;

            return Ok(Self {
                writer: pool.clone(),
                reader: pool,
            });
        }

        let writer_opts = SqliteConnectOptions::from_str(database_url)
            .map_err(|e| ApiError::Internal(format!("Invalid DATABASE_URL: {e}")))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(writer_opts.clone())
            .await
            .map_err(|e| ApiError::Internal(format!("SQLite writer pool failed: {e}")))?;

        let reader = SqlitePoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(writer_opts)
            .await
            .map_err(|e| ApiError::Internal(format!("SQLite reader pool failed: {e}")))?;

        info!("Connected SQLite split pools (WAL mode: 1 writer, 8 readers)");

        Ok(Self { writer, reader })
    }

    pub async fn run_migrations(&self) -> ApiResult<()> {
        sqlx::migrate!("../../migrations")
            .run(&self.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("Database migration failed: {e}")))?;
        Ok(())
    }

    pub async fn health_check(&self) -> ApiResult<()> {
        sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(&self.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("SQLite health check failed: {e}")))?;
        Ok(())
    }
}

fn ensure_sqlite_parent_dir(database_url: &str) -> ApiResult<()> {
    let raw = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .unwrap_or(database_url);

    let path_part = raw.split('?').next().unwrap_or(raw);
    if path_part.is_empty() {
        return Ok(());
    }

    let path = Path::new(path_part);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ApiError::Internal(format!(
                    "Failed to create SQLite directory '{}': {e}",
                    parent.display()
                ))
            })?;
        }
    }
    Ok(())
}
