use super::error::DailyRoutineError;
use std::time::Duration;

use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::Client;
use tracing::error;
use tracing::warn;
use tracing::{info, instrument};

use crate::error::SpannedExt;
use crate::routine::daily::error::ShutdownMinecraftServerError;

#[instrument("wait_until_pod_stopped", skip(client), level = "trace")]
pub(super) async fn wait_until_pod_stopped(
    client: Client,
    namespace: &str,
    pod_name: &str,
    initial_wait: Duration,
    max_wait: Duration,
    max_errors: u64,
) -> Result<(), DailyRoutineError> {
    info!(
        "Waiting {} to {} seconds for pod '{}' to terminate...",
        initial_wait.as_secs(),
        max_wait.as_secs(),
        pod_name
    );
    tokio::time::sleep(initial_wait).await;
    let mut wait_duration = initial_wait;
    let mut errors_count = 0u64;
    let pod_api: Api<Pod> = Api::namespaced(client, namespace);
    loop {
        match pod_api.get_opt(pod_name).await {
            Ok(Some(_)) => {
                info!(
                    "Pod '{}' still exists. Waiting another 5 seconds...",
                    pod_name
                );
                if wait_duration >= max_wait {
                    error!(
                        "Waited more than {} seconds for pod '{}' to terminate.",
                        max_wait.as_secs(),
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
                wait_duration += Duration::from_secs(5);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            Err(e) => {
                warn!("Error while checking pod '{}': {}", pod_name, e);
                warn!("Waiting another 10 seconds before retrying...");
                errors_count += 1;
                if errors_count >= max_errors {
                    error!(
                        "Failed to check pod '{}' status {} times. Aborting wait.",
                        pod_name, max_errors
                    );
                    break Err(e)
                        .with_span_trace()
                        .map_err(DailyRoutineError::KubeClient);
                }
                wait_duration += Duration::from_secs(10);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
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
