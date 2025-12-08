use super::DailyRoutineContext;
use super::error::DailyRoutineError;
use super::scale_statefulset::scale_statefulset_to_zero;
use super::wait_until_pod_stopped::wait_until_pod_stopped;
use std::time::Duration;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::AttachParams;
use tracing::trace_span;
use tracing::{Instrument, error, warn};
use tracing::{info, instrument};

use crate::error::SpannedExt;
use crate::routine::daily::error::ShutdownMinecraftServerError;

impl DailyRoutineContext {
    #[instrument("phase3", skip(self))]
    pub(super) async fn phase_three(&mut self) -> Result<(), DailyRoutineError> {
        info!("Stopping all mcservers...");

        let client = self.client.clone();
        let namespace = self.config.namespace.clone();

        stream::iter(self.config.mcservers.iter_mut())
            .map(|(name, mcserver)| {
                let name = name.clone();
                let mcserver = mcserver.clone();
                let client = client.clone();
                let namespace = namespace.clone();

                async move {
                    let sts_name = mcserver.read().await.name.clone();
                    let rcon_container = mcserver.read().await.rcon_container.clone();
                    let pod_name = format!("{sts_name}-0");

                    let span = trace_span!(
                        "shutdown_mcserver",
                        kubernetes_namespace = %namespace,
                        statefulset_name = %sts_name,
                        pod_name = %pod_name,
                        mcserver_name = %name,
                        rcon_container = %rcon_container,
                    );

                    async move {
                        let result: Result<(), DailyRoutineError> = async {
                            let scaled =
                                scale_statefulset_to_zero(client.clone(), &namespace, &sts_name)
                                    .await?;

                            if !scaled {
                                return Ok(());
                            }

                            let pod_api: Api<Pod> = Api::namespaced(client.clone(), &namespace);

                            let exec_result = pod_api
                                .exec(
                                    &pod_name,
                                    ["rcon-cli", "stop"],
                                    &AttachParams::default()
                                        .container(&rcon_container)
                                )
                                .await;

                            match exec_result {
                                Ok(attached) => {
                                    if let Err(e) = attached
                                        .join()
                                        .await
                                        .map_err(|e| {
                                            ShutdownMinecraftServerError::Exec(Box::new(e))
                                        })
                                            .with_span_trace()
                                        .map_err(|e| {
                                            DailyRoutineError::ShutdownMinecraftServer(
                                                sts_name.clone(),
                                                e,
                                            )
                                        })
                                    {
                                        warn!(
                                            "Failed to join executed stop command on mcserver '{name}' (pod '{}'): {}",
                                            pod_name,
                                            e
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to exec stop command on mcserver '{name}' (pod '{}'): {}",
                                        pod_name,
                                        e
                                    );
                                }
                            }

                            wait_until_pod_stopped(
                                client.clone(),
                                &namespace,
                                &pod_name,
                                Duration::from_secs(10),
                                Duration::from_secs(150),
                                3,
                            )
                            .await?;

                            Ok(())
                        }
                        .await;

                        match result {
                            Ok(()) => Ok(()),
                            Err(e) => {
                                error!("Failed to shutdown mcserver '{name}': {}", e);
                                Err(e)
                            }
                        }
                    }
                    .instrument(span)
                    .await
                }
                .in_current_span()
            })
            .buffer_unordered(10)
            .try_for_each(|_| async { Ok::<(), DailyRoutineError>(()) })
            .await
    }
}
