use k8s_openapi::api::batch::v1::{Job, JobStatus};
use kube::Api;
use kube::Client;
use thiserror::Error;
use tracing::error;
use tracing::warn;
use tracing::{info, instrument};

use crate::config::polling::PollingConfig;
use crate::error::SpannedErr;
use crate::error::SpannedExt;

#[derive(Error, Debug)]
pub enum WaitJobFinishedError {
    #[error("Kubernetes client error: {0}")]
    KubeClient(kube::Error),

    #[error("Job has no 'status' field")]
    JobHasNoStatus,

    #[error("Job did not finish within {0} seconds timeout")]
    JobCompletionCheckTimeout(u64),
}

#[instrument("wait_until_job_finished", skip(client), level = "trace")]
pub(crate) async fn wait_until_job_finished(
    client: Client,
    namespace: &str,
    job_name: &str,
    polling_config: &PollingConfig,
) -> Result<JobStatus, SpannedErr<WaitJobFinishedError>> {
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
            Ok(Job {
                status: Some(status),
                ..
            }) => {
                if status.active.unwrap_or(0) == 0 {
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
                    .with_span_trace();
                }
                wait_duration += polling_config.poll_interval;
                tokio::time::sleep(polling_config.poll_interval).await;
            }
            Ok(_) => break Err(WaitJobFinishedError::JobHasNoStatus).with_span_trace(),
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
                    break Err(WaitJobFinishedError::KubeClient(e)).with_span_trace();
                }
                wait_duration += polling_config.error_wait;
                tokio::time::sleep(polling_config.error_wait).await;
            }
        }
    }
}
