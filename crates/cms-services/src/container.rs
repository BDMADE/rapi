use crate::auth::AuthService;
use crate::content::ContentService;
use crate::governance::GovernanceService;
use crate::integrations::IntegrationsService;
use crate::media::MediaService;
use crate::schema::SchemaService;
use crate::system::SystemService;
use cms_core::{
    ApiResult, AppConfig, CacheBackend, InMemoryCacheBackend, LocalDiskProvider,
    LocalPubSubBackend, PubSubBackend, QueueBackend,
};
use cms_db::{
    AuthRepository, DbPools, DynamicRepository, GovernanceRepository, IntegrationsRepository,
    MediaRepository, PlatformRepository, RedisRuntime, SchemaEngine, SqliteQueueBackend,
};
use cms_plugin_api::PluginRegistry;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

#[derive(Clone)]
pub struct ServiceContainer {
    pub config: Arc<AppConfig>,
    pub db: DbPools,
    pub cache: Arc<dyn CacheBackend>,
    pub queue: Arc<dyn QueueBackend>,
    pub pubsub: Arc<dyn PubSubBackend>,
    pub plugins: PluginRegistry,
    pub system: SystemService,
    pub auth: AuthService,
    pub schema: SchemaService,
    pub content: ContentService,
    pub media: MediaService,
    pub governance: GovernanceService,
    pub integrations: IntegrationsService,
    pub started_at: Instant,
}

impl ServiceContainer {
    pub async fn bootstrap(config: AppConfig) -> ApiResult<Self> {
        config.validate()?;
        let db = DbPools::connect(&config.database_url).await?;
        db.run_migrations().await?;

        let (cache, queue, pubsub): (
            Arc<dyn CacheBackend>,
            Arc<dyn QueueBackend>,
            Arc<dyn PubSubBackend>,
        ) = if let Some(redis_url) = &config.redis_url {
            let redis = RedisRuntime::connect(redis_url).await?;
            (
                Arc::new(redis.clone()),
                Arc::new(redis.clone()),
                Arc::new(redis),
            )
        } else {
            info!("No REDIS_URL set — running in zero-config local mode (Moka cache + SQLite queue)");
            (
                Arc::new(InMemoryCacheBackend::new(10_000)),
                Arc::new(SqliteQueueBackend::new(db.clone())),
                Arc::new(LocalPubSubBackend::new()),
            )
        };

        let plugins = PluginRegistry::new();
        plugins.boot_all().await?;

        let config = Arc::new(config);
        let started_at = Instant::now();
        let platform_repo = PlatformRepository::new(db.clone());
        let system = SystemService::new(
            platform_repo,
            db.clone(),
            cache.clone(),
            queue.clone(),
            started_at,
        );
        let auth_repo = AuthRepository::new(db.clone());
        let auth = AuthService::new(auth_repo, config.clone(), cache.clone(), pubsub.clone());
        let schema_engine = SchemaEngine::new(db.clone());
        let schema = SchemaService::new(schema_engine.clone(), cache.clone(), pubsub.clone());
        let dynamic_repo = DynamicRepository::new(db.clone(), schema_engine);
        let content = ContentService::new(dynamic_repo, schema.clone(), pubsub.clone());
        let storage = Arc::new(LocalDiskProvider::new(&config.upload_dir)?);
        let media_repo = MediaRepository::new(db.clone());
        let media = MediaService::new(media_repo, storage, queue.clone());
        let gov_repo = GovernanceRepository::new(db.clone());
        let governance = GovernanceService::new(
            gov_repo,
            schema.clone(),
            content.clone(),
            cache.clone(),
            queue.clone(),
        );
        let int_repo = IntegrationsRepository::new(db.clone());
        let integrations = IntegrationsService::new(
            int_repo,
            config.clone(),
            auth.clone(),
            schema.clone(),
            content.clone(),
            governance.clone(),
            queue.clone(),
        );

        Ok(Self {
            config,
            db,
            cache,
            queue,
            pubsub,
            plugins,
            system,
            auth,
            schema,
            content,
            media,
            governance,
            integrations,
            started_at,
        })
    }
}
