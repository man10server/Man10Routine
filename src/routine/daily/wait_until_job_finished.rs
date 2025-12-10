use super::error::{DailyRoutineError, WaitJobFinishedError};

use k8s_openapi::api::batch::v1::{Job, JobStatus};
use kube::Api;
use kube::Client;
use tracing::error;
use tracing::warn;
use tracing::{info, instrument};

use crate::config::polling::PollingConfig;
use crate::error::SpannedExt;

#[instrument("wait_until_job_finished", skip(client), level = "trace")]
pub(super) async fn wait_until_job_finished(
    client: Client,
    namespace: &str,
    job_name: &str,
    polling_config: &PollingConfig,
) -> Result<JobStatus, DailyRoutineError> {
    info!(
        "Waiting {} to {} seconds for job '{}' to finish...",
        polling_config.initial_wait.as_secs(),
        polling_config.max_wait.as_secs(),
        job_name
    );
    tokio::time::sleep(polling_config.initial_wait).await;
    let mut wait_duration = polling_config.initial_wait;
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
                    "Job '{}' still running after {} seconds (active: {:?}). Waiting another {} seconds...",
                    job_name,
                    wait_duration.as_secs(),
                    status.active,
                    polling_config.poll_interval.as_secs()
                );
                if wait_duration >= polling_config.max_wait {
                    error!(
                        "Waited more than {} seconds for job '{}' to finish.",
                        wait_duration.as_secs(),
                        job_name
                    );
                    break Err(WaitJobFinishedError::JobCompletionCheckTimeout(
                        wait_duration.as_secs(),
                    ))
                    .with_span_trace()
                    .map_err(|e| DailyRoutineError::WaitJobFinished(job_name.to_string(), e));
                }
                wait_duration += polling_config.poll_interval;
                tokio::time::sleep(polling_config.poll_interval).await;
            }
            Err(e) => {
                warn!("Error while checking job '{}': {}", job_name, e);
                warn!(
                    "Waiting another {} seconds before retrying...",
                    polling_config.error_wait.as_secs()
                );
                errors_count += 1;
                if errors_count >= polling_config.max_errors {
                    error!(
                        "Failed to check job '{}' status {} times. Aborting wait.",
                        job_name, errors_count
                    );
                    break Err(e)
                        .with_span_trace()
                        .map_err(DailyRoutineError::KubeClient);
                }
                wait_duration += polling_config.error_wait;
                tokio::time::sleep(polling_config.error_wait).await;
            }
        }
    }
}
