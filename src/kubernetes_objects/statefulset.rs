use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetStatus};
use kube::api::{Patch, PatchParams};
use kube::{Api, Client};
use tracing::{Instrument, warn};
use tracing::{error, info, instrument, trace_span};
use tracing_error::{ExtractSpanTrace, SpanTrace};

use crate::config::polling::PollingConfig;
use crate::error::{SpannedErr, SpannedExt};
use crate::kubernetes_objects::MANAGEER_ROLE_NAME;

#[derive(thiserror::Error, Debug)]
pub enum StatefulSetScaleError {
    #[error("Kubernetes client error: {0}")]
    KubeClient(SpannedErr<kube::Error>),

    #[error("Exec command error: {0}")]
    Exec(SpannedErr<Box<dyn std::error::Error + Send + Sync + 'static>>),

    #[error("StatefulSet has no 'replicas' field")]
    StatefulSetHasNoReplicas(SpanTrace),

    #[error("Statefulset {0} cannot be scaled")]
    StatefulSetNotScaled(String, SpannedErr<WaitStatefulSetScaleError>),
}

impl ExtractSpanTrace for StatefulSetScaleError {
    fn span_trace(&self) -> Option<&SpanTrace> {
        match self {
            StatefulSetScaleError::KubeClient(e) => e.span_trace(),
            StatefulSetScaleError::Exec(e) => e.span_trace(),
            StatefulSetScaleError::StatefulSetHasNoReplicas(span_trace) => Some(span_trace),
            StatefulSetScaleError::StatefulSetNotScaled(_, e) => e.span_trace(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WaitStatefulSetScaleError {
    #[error("Kubernetes client error: {0}")]
    KubeClient(kube::Error),

    #[error("StatefulSet has no 'status' field")]
    StatefulSetHasNoStatus,

    #[error("StatefulSet did not met condition within {0} seconds timeout")]
    StatefulSetScaledCheckTimeout(u64),
}

#[instrument(
    "scale_statefulset",
    skip(client),
    fields(
        kubernetes_namespace = %namespace,
        statefulset_name = %sts_name,
        target_replicas = target_replicas
    )
)]
pub(crate) async fn scale_statefulset_to_zero(
    client: Client,
    namespace: &str,
    sts_name: &str,
    target_replicas: i32,
) -> Result<bool, StatefulSetScaleError> {
    let api: Api<StatefulSet> = Api::namespaced(client, namespace);

    let sts = async {
        api.get(sts_name)
            .await
            .with_span_trace()
            .map_err(StatefulSetScaleError::KubeClient)
    }
    .instrument(trace_span!(
        "get_statefulset",
        kubernetes_namespace = %namespace,
        statefulset_name = %sts_name
    ))
    .await?;

    match sts.spec.and_then(|s| s.replicas) {
        Some(current_replicas) if current_replicas == target_replicas => {
            warn!(
                "StatefulSet '{}' is already scaled to {} replicas.",
                sts_name, target_replicas
            );
            warn!("Skipping scaling StatefulSet.");
            Ok(false)
        }
        None => Err(StatefulSetScaleError::StatefulSetHasNoReplicas(
            SpanTrace::capture(),
        )),
        Some(_) => {
            async {
                let patch = serde_json::json!({
                    "kind": "StatefulSet",
                    "apiVersion": "apps/v1",
                    "metadata": {
                        "name": sts_name,
                        "namespace": namespace,
                    },
                    "spec": {
                        "replicas": target_replicas
                    }
                });

                let params = PatchParams::apply(MANAGEER_ROLE_NAME);
                api.patch(sts_name, &params, &Patch::Merge(&patch))
                    .await
                    .with_span_trace()
                    .map_err(StatefulSetScaleError::KubeClient)
            }
            .instrument(trace_span!(
                "scale_down_statefulset",
                kubernetes_namespace = %namespace,
                statefulset_name = %sts_name
            ))
            .await?;

            info!("StatefulSet '{sts_name}' scaled to {target_replicas} replicas.");
            Ok(true)
        }
    }
}

#[instrument("wait_until_statefulset_scaled", skip(client), level = "trace")]
pub(crate) async fn wait_until_statefulset_scaled(
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
