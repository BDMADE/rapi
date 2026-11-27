use cms_core::{ApiResult, CacheBackend, PlatformStats, QueueBackend, RuntimeBackendKind};
use cms_db::{DbPools, PlatformRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthDto {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessDto {
    pub status: &'static str,
    pub database: &'static str,
    pub cache_backend: RuntimeBackendKind,
    pub queue_backend: RuntimeBackendKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminInitDto {
    pub has_admin: bool,
    pub version: &'static str,
    pub default_locale: String,
    pub runtime_backend: RuntimeBackendKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SystemOverviewDto {
    pub version: &'static str,
    pub sqlite_journal_mode: &'static str,
    pub cache_backend: RuntimeBackendKind,
    pub queue_backend: RuntimeBackendKind,
    pub uptime_seconds: u64,
    pub has_admin: bool,
    pub default_locale: String,
    pub stats: PlatformStats,
}

#[derive(Clone)]
pub struct SystemService {
    repo: PlatformRepository,
    db: DbPools,
    cache: Arc<dyn CacheBackend>,
    queue: Arc<dyn QueueBackend>,
    started_at: Instant,
}

impl SystemService {
    pub fn new(
        repo: PlatformRepository,
        db: DbPools,
        cache: Arc<dyn CacheBackend>,
        queue: Arc<dyn QueueBackend>,
        started_at: Instant,
    ) -> Self {
        Self {
            repo,
            db,
            cache,
            queue,
            started_at,
        }
    }

    pub fn health(&self) -> HealthDto {
        HealthDto {
            status: "ok",
            service: "rapi-cms",
            version: env!("CARGO_PKG_VERSION"),
            uptime_seconds: self.started_at.elapsed().as_secs(),
        }
    }

    pub async fn readiness(&self) -> ApiResult<ReadinessDto> {
        self.db.health_check().await?;
        self.cache.health_check().await?;

        Ok(ReadinessDto {
            status: "ready",
            database: "sqlite-wal",
            cache_backend: self.cache.backend_kind(),
            queue_backend: self.queue.backend_kind(),
        })
    }

    pub async fn admin_init(&self) -> ApiResult<AdminInitDto> {
        let has_admin = self.repo.has_admin_user().await?;
        let default_locale = self.repo.default_locale().await?;

        Ok(AdminInitDto {
            has_admin,
            version: env!("CARGO_PKG_VERSION"),
            default_locale,
            runtime_backend: self.cache.backend_kind(),
        })
    }

    pub async fn overview(&self) -> ApiResult<SystemOverviewDto> {
        let has_admin = self.repo.has_admin_user().await?;
        let default_locale = self.repo.default_locale().await?;
        let stats = self.repo.fetch_stats().await?;

        Ok(SystemOverviewDto {
            version: env!("CARGO_PKG_VERSION"),
            sqlite_journal_mode: "WAL",
            cache_backend: self.cache.backend_kind(),
            queue_backend: self.queue.backend_kind(),
            uptime_seconds: self.started_at.elapsed().as_secs(),
            has_admin,
            default_locale,
            stats,
        })
    }
}
