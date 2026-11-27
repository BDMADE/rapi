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
        // Phase 4–6 workers plug into specific queue handlers here.
        Ok(())
    }
}
