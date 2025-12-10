use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::apps::v1::StatefulSetStatus;
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
pub enum WaitStatefulSetScaleError {
    #[error("Kubernetes client error: {0}")]
    KubeClient(kube::Error),

    #[error("StatefulSet has no 'status' field")]
    StatefulSetHasNoStatus,

    #[error("StatefulSet did not met condition within {0} seconds timeout")]
    StatefulSetScaledCheckTimeout(u64),
}

#[instrument("wait_until_statefulset_scaled", skip(client), level = "trace")]
pub(super) async fn wait_until_statefulset_scaled(
    client: Client,
    namespace: &str,
    statefulset_name: &str,
    target_replicas: i32,
    polling_config: &PollingConfig,
) -> Result<StatefulSetStatus, SpannedErr<WaitStatefulSetScaleError>> {
    info!(
        "Waiting {} to {} seconds for statefulset '{}' to be scaled...",
        polling_config.initial_wait.as_secs(),
        polling_config.max_wait.as_secs(),
        statefulset_name
    );
    tokio::time::sleep(polling_config.initial_wait).await;
    let mut wait_duration = polling_config.initial_wait;
    let mut errors_count = 0u64;
    let statefulset_api: Api<StatefulSet> = Api::namespaced(client, namespace);
    loop {
        match statefulset_api.get(statefulset_name).await {
            Ok(StatefulSet {
                status: Some(status),
                ..
            }) => {
                if status.current_replicas.unwrap_or(0) == target_replicas
                    && status.available_replicas.unwrap_or(0) == target_replicas
                {
                    info!(
                        "StatefulSet '{}' has been scaled to {} replicas after {} seconds.",
                        statefulset_name,
                        target_replicas,
                        wait_duration.as_secs()
                    );
                    break Ok(status);
                }

                info!(
                    "StatefulSet '{}' still scaling after {} seconds (current status: {:?}). Waiting another {} seconds...",
                    statefulset_name,
                    wait_duration.as_secs(),
                    status,
                    polling_config.poll_interval.as_secs()
                );
                if wait_duration >= polling_config.max_wait {
                    error!(
                        "Waited more than {} seconds for statefulset '{}' to be scaled.",
                        wait_duration.as_secs(),
                        statefulset_name
                    );
                    break Err(WaitStatefulSetScaleError::StatefulSetScaledCheckTimeout(
                        wait_duration.as_secs(),
                    ))
                    .with_span_trace();
                }
                wait_duration += polling_config.poll_interval;
                tokio::time::sleep(polling_config.poll_interval).await;
            }
            Ok(_) => {
                break Err(WaitStatefulSetScaleError::StatefulSetHasNoStatus).with_span_trace();
            }
            Err(e) => {
                warn!("Error while checking pod '{}': {}", statefulset_name, e);
                warn!(
                    "Waiting another {} seconds before retrying...",
                    polling_config.error_wait.as_secs()
                );
                errors_count += 1;
                if errors_count >= polling_config.max_errors {
                    error!(
                        "Failed to check pod '{}' status {} times. Aborting wait.",
                        statefulset_name, errors_count
                    );
                    break Err(WaitStatefulSetScaleError::KubeClient(e)).with_span_trace();
                }
                wait_duration += polling_config.error_wait;
                tokio::time::sleep(polling_config.error_wait).await;
            }
        }
    }
}
