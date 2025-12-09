use super::error::{DailyRoutineError, WaitJobFinishedError};
use std::time::Duration;

use k8s_openapi::api::batch::v1::{Job, JobStatus};
use kube::Api;
use kube::Client;
use tracing::error;
use tracing::warn;
use tracing::{info, instrument};

use crate::error::SpannedExt;

#[instrument("wait_until_job_finished", skip(client), level = "trace")]
pub(super) async fn wait_until_job_finished(
    client: Client,
    namespace: &str,
    job_name: &str,
    initial_wait: Duration,
    max_wait: Duration,
    max_errors: u64,
) -> Result<JobStatus, DailyRoutineError> {
    info!(
        "Waiting {} to {} seconds for job '{}' to finish...",
        initial_wait.as_secs(),
        max_wait.as_secs(),
        job_name
    );
    tokio::time::sleep(initial_wait).await;
    let mut wait_duration = initial_wait;
    let mut errors_count = 0u64;
    let job_api: Api<Job> = Api::namespaced(client, namespace);
    loop {
        match job_api.get(job_name).await {
            Ok(job) => {
                let status = job.status.unwrap_or_default();
                if status.active == Some(0) || status.active.is_none() {
                    info!(
                        "Job '{}' has finished after {} seconds.",
                        job_name,
                        wait_duration.as_secs()
                    );
                    break Ok(status);
                }

                info!(
                    "Job '{}' still running (active: {:?}). Waiting another 5 seconds...",
                    job_name, status.active
                );
                if wait_duration >= max_wait {
                    error!(
                        "Waited more than {} seconds for job '{}' to finish.",
                        max_wait.as_secs(),
                        job_name
                    );
                    break Err(WaitJobFinishedError::JobCompletionCheckTimeout(
                        wait_duration.as_secs(),
                    ))
                    .with_span_trace()
                    .map_err(|e| DailyRoutineError::WaitJobFinished(job_name.to_string(), e));
                }
                wait_duration += Duration::from_secs(5);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            Err(e) => {
                warn!("Error while checking job '{}': {}", job_name, e);
                warn!("Waiting another 10 seconds before retrying...");
                errors_count += 1;
                if errors_count >= max_errors {
                    error!(
                        "Failed to check job '{}' status {} times. Aborting wait.",
                        job_name, max_errors
                    );
                    break Err(e)
                        .with_span_trace()
                        .map_err(DailyRoutineError::KubeClient);
                }
                wait_duration += Duration::from_secs(10);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }
}
