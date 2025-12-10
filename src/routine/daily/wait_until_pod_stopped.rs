use super::error::DailyRoutineError;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::Client;
use tracing::error;
use tracing::warn;
use tracing::{info, instrument};

use crate::config::polling::PollingConfig;
use crate::error::SpannedExt;
use crate::routine::daily::error::ShutdownMinecraftServerError;

#[instrument("wait_until_pod_stopped", skip(client), level = "trace")]
pub(super) async fn wait_until_pod_stopped(
    client: Client,
    namespace: &str,
    pod_name: &str,
    polling_config: &PollingConfig,
) -> Result<(), DailyRoutineError> {
    info!(
        "Waiting {} to {} seconds for pod '{}' to terminate...",
        polling_config.initial_wait.as_secs(),
        polling_config.max_wait.as_secs(),
        pod_name
    );
    tokio::time::sleep(polling_config.initial_wait).await;
    let mut wait_duration = polling_config.initial_wait;
    let mut errors_count = 0u64;
    let pod_api: Api<Pod> = Api::namespaced(client, namespace);
    loop {
        match pod_api.get_opt(pod_name).await {
            Ok(Some(_)) => {
                info!(
                    "Pod '{}' still exists after {} seconds. Waiting another {} seconds...",
                    pod_name,
                    wait_duration.as_secs(),
                    polling_config.poll_interval.as_secs()
                );
                if wait_duration >= polling_config.max_wait {
                    error!(
                        "Waited more than {} seconds for pod '{}' to terminate.",
                        wait_duration.as_secs(),
                        pod_name
                    );
                    break Err(ShutdownMinecraftServerError::PodShutdownCheckTimeout(
                        wait_duration.as_secs(),
                    ))
                    .with_span_trace()
                    .map_err(|e| {
                        DailyRoutineError::ShutdownMinecraftServer(pod_name.to_string(), e)
                    });
                }
                wait_duration += polling_config.poll_interval;
                tokio::time::sleep(polling_config.poll_interval).await;
            }
            Err(e) => {
                warn!("Error while checking pod '{}': {}", pod_name, e);
                warn!(
                    "Waiting another {} seconds before retrying...",
                    polling_config.error_wait.as_secs()
                );
                errors_count += 1;
                if errors_count >= polling_config.max_errors {
                    error!(
                        "Failed to check pod '{}' status {} times. Aborting wait.",
                        pod_name, errors_count
                    );
                    break Err(e)
                        .with_span_trace()
                        .map_err(DailyRoutineError::KubeClient);
                }
                wait_duration += polling_config.error_wait;
                tokio::time::sleep(polling_config.error_wait).await;
            }
            Ok(None) => {
                info!(
                    "Pod '{}' has been terminated after {} seconds.",
                    pod_name,
                    wait_duration.as_secs()
                );
                break Ok(());
            }
        }
    }
}
