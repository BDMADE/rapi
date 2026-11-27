use async_trait::async_trait;
use cms_core::{
    ApiError, ApiResult, CacheBackend, InvalidationEvent, JobMessage, PubSubBackend, QueueBackend,
    RuntimeBackendKind,
};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn};

const PUBSUB_CHANNEL: &str = "rapi:pubsub:invalidations";

#[derive(Clone)]
pub struct RedisRuntime {
    manager: ConnectionManager,
    local_bus: broadcast::Sender<InvalidationEvent>,
}

impl RedisRuntime {
    pub async fn connect(redis_url: &str) -> ApiResult<Self> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| ApiError::Internal(format!("Invalid REDIS_URL: {e}")))?;
        let manager = ConnectionManager::new(client.clone())
            .await
            .map_err(|e| ApiError::Internal(format!("Redis connection failed: {e}")))?;

        let (local_bus, _) = broadcast::channel(256);
        let bus_clone = local_bus.clone();

        // Spawn subscriber keeping Redis invalidation channel subscribed
        tokio::spawn(async move {
            match client.get_async_pubsub().await {
                Ok(mut pubsub) => {
                    if let Err(e) = pubsub.subscribe(PUBSUB_CHANNEL).await {
                        warn!("Failed to subscribe to Redis invalidation channel: {e}");
                        return;
                    }
                    let _keepalive = pubsub;
                    let _ = &bus_clone;
                    loop {
                        tokio::time::sleep(Duration::from_secs(3600)).await;
                    }
                }
                Err(e) => {
                    warn!("Redis pub/sub listener unavailable: {e}");
                }
            }
        });

        info!("Connected Redis runtime backend for cache & queues");
        Ok(Self { manager, local_bus })
    }
}

#[async_trait]
impl CacheBackend for RedisRuntime {
    fn backend_kind(&self) -> RuntimeBackendKind {
        RuntimeBackendKind::Redis
    }

    async fn get(&self, key: &str) -> ApiResult<Option<String>> {
        let mut conn = self.manager.clone();
        conn.get(key)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis GET error: {e}")))
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> ApiResult<()> {
        let mut conn = self.manager.clone();
        if let Some(ttl) = ttl {
            let secs = ttl.as_secs().max(1);
            let _: () = conn
                .set_ex(key, value, secs)
                .await
                .map_err(|e| ApiError::Internal(format!("Redis SETEX error: {e}")))?;
        } else {
            let _: () = conn
                .set(key, value)
                .await
                .map_err(|e| ApiError::Internal(format!("Redis SET error: {e}")))?;
        }
        Ok(())
    }

    async fn delete(&self, key: &str) -> ApiResult<()> {
        let mut conn = self.manager.clone();
        let _: () = conn
            .del(key)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis DEL error: {e}")))?;
        Ok(())
    }

    async fn health_check(&self) -> ApiResult<()> {
        let mut conn = self.manager.clone();
        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis PING failed: {e}")))?;
        if pong == "PONG" {
            Ok(())
        } else {
            Err(ApiError::Internal(format!("Unexpected Redis PING response: {pong}")))
        }
    }
}

#[async_trait]
impl QueueBackend for RedisRuntime {
    fn backend_kind(&self) -> RuntimeBackendKind {
        RuntimeBackendKind::Redis
    }

    async fn enqueue(&self, job: JobMessage) -> ApiResult<()> {
        let mut conn = self.manager.clone();
        let raw = serde_json::to_string(&job)
            .map_err(|e| ApiError::Internal(format!("Serialize job failed: {e}")))?;
        let _: () = conn
            .rpush(&job.queue, raw)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis RPUSH error: {e}")))?;
        Ok(())
    }

    async fn dequeue(&self, queue: &str) -> ApiResult<Option<JobMessage>> {
        let mut conn = self.manager.clone();
        let raw: Option<String> = conn
            .lpop(queue, None)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis LPOP error: {e}")))?;
        match raw {
            Some(s) => {
                let mut job: JobMessage = serde_json::from_str(&s)
                    .map_err(|e| ApiError::Internal(format!("Decode Redis job failed: {e}")))?;
                job.attempts += 1;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    async fn ack(&self, _queue: &str, _job_id: &str) -> ApiResult<()> {
        Ok(())
    }

    async fn fail(&self, queue: &str, job_id: &str, error: &str) -> ApiResult<()> {
        let mut conn = self.manager.clone();
        let dlq = format!("{queue}:dlq");
        let entry = serde_json::json!({ "jobId": job_id, "error": error }).to_string();
        let _: () = conn
            .rpush(dlq, entry)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis DLQ push failed: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl PubSubBackend for RedisRuntime {
    fn backend_kind(&self) -> RuntimeBackendKind {
        RuntimeBackendKind::Redis
    }

    async fn publish(&self, event: InvalidationEvent) -> ApiResult<()> {
        let mut conn = self.manager.clone();
        let payload = serde_json::to_string(&event)
            .map_err(|e| ApiError::Internal(format!("Invalidation serialize failed: {e}")))?;
        let _: () = conn
            .publish(PUBSUB_CHANNEL, payload)
            .await
            .map_err(|e| ApiError::Internal(format!("Redis PUBLISH failed: {e}")))?;
        let _ = self.local_bus.send(event);
        Ok(())
    }

    fn subscribe_local(&self) -> broadcast::Receiver<InvalidationEvent> {
        self.local_bus.subscribe()
    }
}
