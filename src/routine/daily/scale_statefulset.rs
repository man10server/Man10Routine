use k8s_openapi::api::apps::v1::StatefulSet;
use kube::Api;
use kube::api::PatchParams;
use tracing::{Instrument, warn};
use tracing::{info, instrument, trace_span};
use tracing_error::SpanTrace;

use crate::error::SpannedExt;
use crate::kubernetes_objects::MANAGEER_ROLE_NAME;
use crate::routine::daily::error::StatefulSetScaleError;

#[instrument(
    "scale_statefulset",
    skip(client),
    fields(
        kubernetes_namespace = %namespace,
        statefulset_name = %sts_name,
        target_replicas = target_replicas
    )
)]
pub(super) async fn scale_statefulset_to_zero(
    client: kube::Client,
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
                api.patch(sts_name, &params, &kube::api::Patch::Merge(&patch))
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
