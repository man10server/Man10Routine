use super::DailyRoutineContext;
use super::error::DailyRoutineError;
use std::time::Duration;

use k8s_openapi::api::apps::v1::StatefulSet;
use kube::Api;
use kube::api::PatchParams;
use tracing::trace_span;
use tracing::warn;
use tracing::{Instrument, error};
use tracing::{info, instrument};
use tracing_error::TracedError;

use crate::kubernetes_objects::MANAGEER_ROLE_NAME;
use crate::routine::daily::error::ShutdownMinecraftServerError;

impl DailyRoutineContext {
    #[instrument("phase2", skip(self))]
    pub(super) async fn phase_two(&mut self) -> Result<(), DailyRoutineError> {
        let proxy_sts_name = &self.config.mcproxy.read().await.name;
        info!("Stopping proxy server...");

        let api: Api<StatefulSet> = Api::namespaced(self.client.clone(), &self.config.namespace);

        let proxy_sts = async {
            api.get(proxy_sts_name)
                .await
                .map_err(ShutdownMinecraftServerError::KubeClient)
                .map_err(TracedError::from)
                .map_err(|e| DailyRoutineError::ShutdownMinecraftServer(proxy_sts_name.clone(), e))
        }
        .instrument(trace_span!(
            "get_proxy_statefulset",
            kubernetes_namespace = %self.config.namespace,
            statefulset_name = %proxy_sts_name
        ))
        .await?;

        match proxy_sts.spec.and_then(|s| s.replicas) {
            Some(0) => {
                warn!(
                    "Proxy server StatefulSet '{}' is already scaled to 0 replicas.",
                    proxy_sts_name
                );
                warn!("Skipping scaling down proxy server.");
                Ok(())
            }
            None => {
                error!(
                    "Proxy server StatefulSet '{}' has no replicas field set.",
                    proxy_sts_name
                );
                Err(TracedError::from(
                    ShutdownMinecraftServerError::StatefulSetNoReplicas,
                ))
                .map_err(|e| DailyRoutineError::ShutdownMinecraftServer(proxy_sts_name.clone(), e))
            }
            Some(_) => {
                async {
                    let patch = serde_json::json!({

                        "kind": "StatefulSet",
                        "apiVersion": "apps/v1",
                        "metadata": {
                            "name": proxy_sts_name,
                            "namespace": self.config.namespace,
                        },
                        "spec": {
                            "replicas": 0
                        }
                    });

                    let params = PatchParams::apply(MANAGEER_ROLE_NAME);
                    api.patch(proxy_sts_name, &params, &kube::api::Patch::Merge(&patch))
                        .await
                        .map_err(ShutdownMinecraftServerError::KubeClient)
                        .map_err(TracedError::from)
                        .map_err(|e| {
                            DailyRoutineError::ShutdownMinecraftServer(proxy_sts_name.clone(), e)
                        })
                }
                .instrument(trace_span!(
                    "scale_down_proxy_statefulset",
                    kubernetes_namespace = %self.config.namespace,
                    statefulset_name = %proxy_sts_name
                ))
                .await?;

                info!("Proxy server '{proxy_sts_name}' scaled down to 0 replicas.");
                self.wait_until_pod_stopped(
                    format!("{}-0", proxy_sts_name).as_str(),
                    Duration::from_secs(60),
                    Duration::from_secs(150),
                    5,
                )
                .await
            }
        }
    }
}
