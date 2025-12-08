use std::time::Duration;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::Client;
use kube::api::PatchParams;
use thiserror::Error;
use tracing::trace_span;
use tracing::warn;
use tracing::{Instrument, error};
use tracing::{info, instrument};
use tracing_error::TracedError;
use tracing_error::{ExtractSpanTrace, SpanTrace};

use crate::config::Config;
use crate::kubernetes_objects::MANAGEER_ROLE_NAME;
use crate::kubernetes_objects::argocd::ArgoCdError;
use crate::kubernetes_objects::minecraft_chart::MinecraftChartError;

#[derive(Error, Debug)]
pub enum DailyRoutineError {
    #[error("ArgoCD error: {0}")]
    ArgoCd(#[from] ArgoCdError),

    #[error("Minecraft Chart error: {0}")]
    MinecraftChart(#[from] MinecraftChartError),

    #[error("Minecraft Server {0} cannot be shutdown: {1}")]
    ShutdownMinecraftServer(String, TracedError<ShutdownMinecraftServerError>),

    #[error("Kubernetes client error: {0}")]
    KubeClient(#[from] TracedError<kube::Error>),
}

#[derive(Error, Debug)]
pub enum ShutdownMinecraftServerError {
    #[error("Kubernetes client error: {0}")]
    KubeClient(kube::Error),

    #[error("Failed to scale Statefulset: the statefulset has no 'replicas' field")]
    StatefulSetNoReplicas,

    #[error("Pod did not shutdown within {0} seconds timeout")]
    PodShutdownCheckTimeout(u64),
}

impl ExtractSpanTrace for DailyRoutineError {
    fn span_trace(&self) -> Option<&SpanTrace> {
        match self {
            DailyRoutineError::ArgoCd(e) => e.span_trace(),
            DailyRoutineError::MinecraftChart(e) => e.span_trace(),
            DailyRoutineError::ShutdownMinecraftServer(_, e) => {
                (e as &(dyn std::error::Error + 'static)).span_trace()
            }
            DailyRoutineError::KubeClient(e) => {
                (e as &(dyn std::error::Error + 'static)).span_trace()
            }
        }
    }
}

pub(crate) struct DailyRoutineContext {
    config: Config,
    client: Client,
}

impl DailyRoutineContext {
    pub(crate) fn new(config: Config, client: Client) -> DailyRoutineContext {
        DailyRoutineContext { config, client }
    }

    #[instrument("daily_routine", skip(self))]
    pub(crate) async fn run(&mut self) -> Result<(), DailyRoutineError> {
        info!("Starting daily routine...");

        {
            let result = self.phase_one().await;
            if result.is_err() {
                return self.finalizer(result).await;
            }
        }

        // Sleep for 10 seconds
        info!("Phase1 completed. Sleeping for 10 seconds before continuing...");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        {
            let result = self.phase_two().await;
            if result.is_err() {
                return self.finalizer(result).await;
            }
        }

        info!("Daily routine completed successfully.");
        self.finalizer(Ok(())).await
    }

    #[instrument("finalizer", skip(self, result))]
    async fn finalizer(
        &mut self,
        result: Result<(), DailyRoutineError>,
    ) -> Result<(), DailyRoutineError> {
        info!("Tearup all ArgoCD applications of minecraft charts...");
        if let Err(e) = self.config.mcproxy.write().await.release().await {
            error!("Failed to release mcproxy: {}", e);
            if let Some(span_trace) = e.span_trace() {
                eprintln!("\n{}\n", color_spantrace::colorize(span_trace));
            }
        }
        for (name, mcserver) in self.config.mcservers.iter_mut() {
            if let Err(e) = mcserver.write().await.release().await {
                error!("Failed to release mcserver '{name}': {}", e);
                if let Some(span_trace) = e.span_trace() {
                    eprintln!("\n{}\n", color_spantrace::colorize(span_trace));
                }
            }
        }

        result
    }

    #[instrument("phase1", skip(self))]
    async fn phase_one(&mut self) -> Result<(), DailyRoutineError> {
        info!("Teardown all ArgoCD applications of minecraft charts...");
        self.config
            .mcproxy
            .write()
            .await
            .argocd_teardown(self.client.clone())
            .await?;
        info!("Teardown all mcservers...");
        stream::iter(self.config.mcservers.iter_mut())
            .map(|(name, mcserver)| {
                let name = name.clone();
                let client = self.client.clone();
                let mcserver = mcserver.clone();
                async move {
                    match mcserver.write().await.argocd_teardown(client).await {
                        Ok(_) => Ok(()),
                        Err(e) => {
                            error!("Failed to teardown mcserver '{name}': {}", e);
                            Err(e)
                        }
                    }
                }
                .in_current_span()
            })
            .buffer_unordered(10)
            .try_for_each(|_| async { Ok::<(), MinecraftChartError>(()) })
            .await
            .map_err(DailyRoutineError::from)?;
        Ok(())
    }

    #[instrument("phase2", skip(self))]
    async fn phase_two(&mut self) -> Result<(), DailyRoutineError> {
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

    #[instrument("wait_until_pod_stopped", skip(self), level = "trace")]
    async fn wait_until_pod_stopped(
        &self,
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
        let pod_api: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);
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
                        break Err(TracedError::from(
                            ShutdownMinecraftServerError::PodShutdownCheckTimeout(
                                wait_duration.as_secs(),
                            ),
                        ))
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
                        break Err(DailyRoutineError::from(TracedError::from(e)));
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
}
