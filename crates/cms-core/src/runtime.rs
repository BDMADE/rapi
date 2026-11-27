use crate::error::{ApiError, ApiResult};
use async_trait::async_trait;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeBackendKind {
    Local,
    Redis,
}

impl RuntimeBackendKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Redis => "redis",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobMessage {
    pub id: String,
    pub queue: String,
    pub payload: serde_json::Value,
    pub attempts: u32,
}

impl JobMessage {
    pub fn new(queue: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::now_v7().to_string(),
            queue: queue.into(),
            payload,
            attempts: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvalidationEvent {
    SchemaUpdated { uid: String },
    PermissionsUpdated { subject: String },
    DocumentChanged { uid: String, document_id: String },
}

#[async_trait]
pub trait CacheBackend: Send + Sync {
    fn backend_kind(&self) -> RuntimeBackendKind;
    async fn get(&self, key: &str) -> ApiResult<Option<String>>;
    async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> ApiResult<()>;
    async fn delete(&self, key: &str) -> ApiResult<()>;
    async fn health_check(&self) -> ApiResult<()>;
}

#[async_trait]
pub trait QueueBackend: Send + Sync {
    fn backend_kind(&self) -> RuntimeBackendKind;
    async fn enqueue(&self, job: JobMessage) -> ApiResult<()>;
    async fn dequeue(&self, queue: &str) -> ApiResult<Option<JobMessage>>;
    async fn ack(&self, queue: &str, job_id: &str) -> ApiResult<()>;
    async fn fail(&self, queue: &str, job_id: &str, error: &str) -> ApiResult<()>;
}

#[async_trait]
pub trait PubSubBackend: Send + Sync {
    fn backend_kind(&self) -> RuntimeBackendKind;
    async fn publish(&self, event: InvalidationEvent) -> ApiResult<()>;
    fn subscribe_local(&self) -> broadcast::Receiver<InvalidationEvent>;
}

#[derive(Clone)]
struct CacheEntry {
    value: String,
    expires_at: Option<Instant>,
}

/// Zero-config in-memory cache powered by `moka` with strict per-entry TTL enforcement.
#[derive(Clone)]
pub struct InMemoryCacheBackend {
    cache: Cache<String, CacheEntry>,
}

impl InMemoryCacheBackend {
    pub fn new(max_capacity: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(Duration::from_secs(3600))
            .build();
        Self { cache }
    }
}

#[async_trait]
impl CacheBackend for InMemoryCacheBackend {
    fn backend_kind(&self) -> RuntimeBackendKind {
        RuntimeBackendKind::Local
    }

    async fn get(&self, key: &str) -> ApiResult<Option<String>> {
        let Some(entry) = self.cache.get(key).await else {
            return Ok(None);
        };
        if let Some(exp) = entry.expires_at {
            if Instant::now() >= exp {
                self.cache.invalidate(key).await;
                return Ok(None);
            }
        }
        Ok(Some(entry.value))
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> ApiResult<()> {
        let expires_at = ttl.map(|d| Instant::now() + d);
        self.cache
            .insert(
                key.to_string(),
                CacheEntry {
                    value: value.to_string(),
                    expires_at,
                },
            )
            .await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> ApiResult<()> {
        self.cache.invalidate(key).await;
        Ok(())
    }

    async fn health_check(&self) -> ApiResult<()> {
        Ok(())
    }
}

/// Zero-config in-process pub/sub powered by `tokio::sync::broadcast`.
#[derive(Clone)]
pub struct LocalPubSubBackend {
    sender: broadcast::Sender<InvalidationEvent>,
}

impl Default for LocalPubSubBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalPubSubBackend {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }
}

#[async_trait]
impl PubSubBackend for LocalPubSubBackend {
    fn backend_kind(&self) -> RuntimeBackendKind {
        RuntimeBackendKind::Local
    }

    async fn publish(&self, event: InvalidationEvent) -> ApiResult<()> {
        // Ignore error if there are currently zero active subscribers
        let _ = self.sender.send(event).map_err(|e| ApiError::Internal(e.to_string()));
        Ok(())
    }

    fn subscribe_local(&self) -> broadcast::Receiver<InvalidationEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_cache_enforces_per_entry_ttl() {
        let cache = InMemoryCacheBackend::new(100);
        cache
            .set("short", "val1", Some(Duration::from_millis(40)))
            .await
            .unwrap();
        cache
            .set("long", "val2", Some(Duration::from_secs(60)))
            .await
            .unwrap();

        assert_eq!(cache.get("short").await.unwrap().as_deref(), Some("val1"));
        tokio::time::sleep(Duration::from_millis(70)).await;
        assert_eq!(cache.get("short").await.unwrap(), None);
        assert_eq!(cache.get("long").await.unwrap().as_deref(), Some("val2"));
    }
}
