use crate::error::{ApiError, ApiResult};
use async_trait::async_trait;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::time::Duration;
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

/// Zero-config in-memory cache powered by `moka`.
#[derive(Clone)]
pub struct InMemoryCacheBackend {
    cache: Cache<String, String>,
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
        Ok(self.cache.get(key).await)
    }

    async fn set(&self, key: &str, value: &str, _ttl: Option<Duration>) -> ApiResult<()> {
        self.cache.insert(key.to_string(), value.to_string()).await;
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
