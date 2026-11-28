use cms_core::{ApiResult, JobMessage};
use cms_services::ServiceContainer;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, error, info};

pub const QUEUES: &[&str] = &[
    "rapi:jobs:webhook",
    "rapi:jobs:media",
    "rapi:jobs:email",
    "rapi:jobs:release",
    "rapi:jobs:index",
];

pub struct WorkerRunner {
    services: ServiceContainer,
}

impl WorkerRunner {
    pub fn new(services: ServiceContainer) -> Self {
        Self { services }
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) -> ApiResult<()> {
        info!(
            backend = self.services.queue.backend_kind().as_str(),
            "Background worker started"
        );

        loop {
            if *shutdown.borrow() {
                info!("Background worker shutting down cleanly");
                break;
            }

            let mut processed_any = false;

            for queue in QUEUES {
                match self.services.queue.dequeue(queue).await {
                    Ok(Some(job)) => {
                        processed_any = true;
                        match self.handle_job(&job).await {
                            Ok(()) => {
                                let _ = self.services.queue.ack(queue, &job.id).await;
                            }
                            Err(err) => {
                                error!(job_id = %job.id, queue = *queue, error = %err, "Job execution failed");
                                let _ = self
                                    .services
                                    .queue
                                    .fail(queue, &job.id, &err.to_string())
                                    .await;
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        error!(queue = *queue, error = %err, "Queue poll error");
                    }
                }
            }

            // Sweep any scheduled releases whose `scheduled_at <= now`
            if let Err(err) = self.services.governance.process_due_releases().await {
                error!(error = %err, "Scheduled release sweep failed");
            }

            if !processed_any {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(750)) => {}
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            info!("Background worker shutting down cleanly");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_job(&self, job: &JobMessage) -> ApiResult<()> {
        debug!(job_id = %job.id, queue = %job.queue, "Dispatching background job");
        match job.queue.as_str() {
            "rapi:jobs:webhook" => {
                self.services.integrations.process_webhook_job(job).await?;
            }
            "rapi:jobs:media" => {
                if let Some(file_id) = job.payload.get("fileId").and_then(|v| v.as_i64()) {
                    self.services.media.process_responsive_formats(file_id).await?;
                }
            }
            "rapi:jobs:email" => {
                self.services.integrations.process_email_job(job).await?;
            }
            "rapi:jobs:release" => {
                let _ = self.services.governance.process_due_releases().await?;
            }
            _ => {}
        }
        Ok(())
    }
}
