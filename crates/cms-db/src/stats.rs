use crate::pool::DbPools;
use cms_core::{ApiError, ApiResult, PlatformStats};
use sqlx::Row;

#[derive(Clone)]
pub struct PlatformRepository {
    pools: DbPools,
}

impl PlatformRepository {
    pub fn new(pools: DbPools) -> Self {
        Self { pools }
    }

    pub async fn has_admin_user(&self) -> ApiResult<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_users")
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed querying admin_users: {e}")))?;
        Ok(count > 0)
    }

    pub async fn default_locale(&self) -> ApiResult<String> {
        let locale: Option<String> =
            sqlx::query_scalar("SELECT code FROM locales WHERE is_default = 1 LIMIT 1")
                .fetch_optional(&self.pools.reader)
                .await
                .map_err(|e| ApiError::Internal(format!("Failed querying default locale: {e}")))?;
        Ok(locale.unwrap_or_else(|| "en".to_string()))
    }

    pub async fn fetch_stats(&self) -> ApiResult<PlatformStats> {
        let row = sqlx::query(
            r#"
            SELECT
                (SELECT COUNT(*) FROM content_types WHERE kind != 'component') AS content_types,
                (SELECT COUNT(DISTINCT document_id) FROM documents) AS documents,
                (SELECT COUNT(*) FROM documents WHERE status = 'published') AS published_documents,
                (SELECT COUNT(*) FROM upload_files) AS media_files,
                (SELECT COUNT(*) FROM locales) AS locales,
                (SELECT COUNT(*) FROM admin_users) AS admin_users,
                (SELECT COUNT(*) FROM webhooks) AS webhooks,
                (SELECT COUNT(*) FROM job_queue WHERE status = 'pending') AS pending_jobs
            "#,
        )
        .fetch_one(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed querying platform stats: {e}")))?;

        Ok(PlatformStats {
            content_types: row.get("content_types"),
            documents: row.get("documents"),
            published_documents: row.get("published_documents"),
            media_files: row.get("media_files"),
            locales: row.get("locales"),
            admin_users: row.get("admin_users"),
            webhooks: row.get("webhooks"),
            pending_jobs: row.get("pending_jobs"),
        })
    }
}
