use crate::pool::DbPools;
use async_trait::async_trait;
use cms_core::{ApiError, ApiResult, JobMessage, QueueBackend, RuntimeBackendKind};
use sqlx::Row;

/// Persistent SQLite-backed job queue for zero-config single-binary operation.
#[derive(Clone)]
pub struct SqliteQueueBackend {
    pools: DbPools,
}

impl SqliteQueueBackend {
    pub fn new(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl QueueBackend for SqliteQueueBackend {
    fn backend_kind(&self) -> RuntimeBackendKind {
        RuntimeBackendKind::Local
    }

    async fn enqueue(&self, job: JobMessage) -> ApiResult<()> {
        let payload_json = serde_json::to_string(&job.payload)
            .map_err(|e| ApiError::Internal(format!("Job serialize error: {e}")))?;

        sqlx::query(
            "INSERT INTO job_queue (id, queue, payload_json, status, attempts) VALUES (?, ?, ?, 'pending', ?)",
        )
        .bind(&job.id)
        .bind(&job.queue)
        .bind(payload_json)
        .bind(job.attempts as i64)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("Job enqueue failed: {e}")))?;

        Ok(())
    }

    async fn dequeue(&self, queue: &str) -> ApiResult<Option<JobMessage>> {
        let row = sqlx::query(
            r#"
            UPDATE job_queue
            SET status = 'processing',
                attempts = attempts + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = (
                SELECT id FROM job_queue
                WHERE queue = ?
                  AND status = 'pending'
                  AND available_at <= strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                ORDER BY created_at ASC
                LIMIT 1
            )
            RETURNING id, queue, payload_json, attempts
            "#,
        )
        .bind(queue)
        .fetch_optional(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("Job dequeue failed: {e}")))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let id: String = row.get("id");
        let queue_name: String = row.get("queue");
        let payload_str: String = row.get("payload_json");
        let attempts: i64 = row.get("attempts");
        let payload: serde_json::Value = serde_json::from_str(&payload_str)
            .map_err(|e| ApiError::Internal(format!("Job payload decode failed: {e}")))?;

        Ok(Some(JobMessage {
            id,
            queue: queue_name,
            payload,
            attempts: attempts as u32,
        }))
    }

    async fn ack(&self, _queue: &str, job_id: &str) -> ApiResult<()> {
        sqlx::query(
            "UPDATE job_queue SET status = 'completed', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(job_id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("Job ack failed: {e}")))?;
        Ok(())
    }

    async fn fail(&self, _queue: &str, job_id: &str, error: &str) -> ApiResult<()> {
        sqlx::query(
            r#"
            UPDATE job_queue
            SET status = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END,
                last_error = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
            "#,
        )
        .bind(error)
        .bind(job_id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("Job fail update failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sqlite_queue_roundtrip() {
        let pools = DbPools::connect("sqlite::memory:").await.unwrap();
        pools.run_migrations().await.unwrap();
        let queue = SqliteQueueBackend::new(pools);

        let msg = JobMessage::new("rapi:jobs:webhook", serde_json::json!({ "event": "entry.publish" }));
        let job_id = msg.id.clone();
        queue.enqueue(msg).await.unwrap();

        let claimed = queue.dequeue("rapi:jobs:webhook").await.unwrap().expect("expected job");
        assert_eq!(claimed.id, job_id);
        assert_eq!(claimed.attempts, 1);

        queue.ack("rapi:jobs:webhook", &job_id).await.unwrap();
        assert!(queue.dequeue("rapi:jobs:webhook").await.unwrap().is_none());
    }
}
