use anyhow::Context;
use clap::{Parser, Subcommand};
use cms_api::build_router;
use cms_core::AppConfig;
use cms_db::DbPools;
use cms_services::ServiceContainer;
use cms_worker::WorkerRunner;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "rapi", version, about = "RAPI — Rust Headless CMS (Strapi v5 feature parity)")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start the HTTP API server (and embedded worker by default)
    Serve {
        /// Disable embedded background worker (use when running dedicated `rapi worker` processes)
        #[arg(long, default_value_t = false)]
        no_worker: bool,
    },
    /// Run a dedicated background queue consumer process
    Worker,
    /// Apply core SQLite platform migrations and exit
    Migrate,
    /// Print active runtime info and platform database statistics
    Info,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    init_tracing(&config.log_format);

    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Commands::Serve { no_worker: false });

    match command {
        Commands::Migrate => {
            let pools = DbPools::connect(&config.database_url)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            pools
                .run_migrations()
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            info!("Core SQLite migrations applied successfully");
        }
        Commands::Info => {
            let services = ServiceContainer::bootstrap(config)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let overview = services
                .system
                .overview()
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            println!("{}", serde_json::to_string_pretty(&overview)?);
        }
        Commands::Worker => {
            let services = ServiceContainer::bootstrap(config)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let (shutdown_tx, shutdown_rx) = watch::channel(false);
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                let _ = shutdown_tx.send(true);
            });
            let worker = WorkerRunner::new(services);
            worker
                .run(shutdown_rx)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        }
        Commands::Serve { no_worker } => {
            let addr = config.socket_addr();
            let public_url = config.public_url.clone();
            std::fs::create_dir_all(&config.upload_dir)
                .context("Failed creating upload directory")?;

            let services = ServiceContainer::bootstrap(config)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;

            let (shutdown_tx, shutdown_rx) = watch::channel(false);

            if !no_worker {
                let worker_services = services.clone();
                let worker_rx = shutdown_rx.clone();
                tokio::spawn(async move {
                    let worker = WorkerRunner::new(worker_services);
                    let _ = worker.run(worker_rx).await;
                });
            }

            let app = build_router(services);
            let listener = TcpListener::bind(&addr)
                .await
                .with_context(|| format!("Failed to bind HTTP listener on {addr}"))?;

            info!("RAPI CMS listening on {public_url} (bound to {addr})");

            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = tokio::signal::ctrl_c().await;
                    info!("Shutdown signal received");
                    let _ = shutdown_tx.send(true);
                })
                .await
                .context("HTTP server error")?;
        }
    }

    Ok(())
}

fn init_tracing(format: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,cms_api=debug,cms_db=info"));

    if format.eq_ignore_ascii_case("json") {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .compact()
            .init();
    }
}
