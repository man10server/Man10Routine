use std::time::Duration;

use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::AttachParams;
use tracing::{Instrument, error, trace_span, warn};
use tracing::{info, instrument};

use crate::error::SpannedExt;
use crate::routine::daily::error::DailyRoutineError;
use crate::routine::daily::error::ShutdownMinecraftServerError;
use crate::scheduler::TaskSpec;

use super::DailyRoutineContext;
use super::scale_statefulset::scale_statefulset_to_zero;
use super::wait_until_pod_stopped::wait_until_pod_stopped;

#[instrument("phase_shutdown_mcserver", skip(ctx))]
async fn shutdown_mcserver(
    ctx: DailyRoutineContext,
    mcserver_name: String,
) -> Result<(), DailyRoutineError> {
    let client = ctx.client.clone();
    let namespace = ctx.config.namespace.clone();
    let mcserver = ctx
        .config
        .mcservers
        .get(&mcserver_name)
        .cloned()
        .expect("mcserver must exist in config");

    let (sts_name, rcon_container) = {
        let read = mcserver.read().await;
        (read.name.clone(), read.rcon_container.clone())
    };
    let pod_name = format!("{sts_name}-0");

    let span = trace_span!(
        "shutdown_mcserver",
        kubernetes_namespace = %namespace,
        statefulset_name = %sts_name,
        pod_name = %pod_name,
        mcserver_name = %mcserver_name,
        rcon_container = %rcon_container,
    );

    async move {
        let result: Result<(), DailyRoutineError> = async {
            let scaled = scale_statefulset_to_zero(client.clone(), &namespace, &sts_name).await?;

            if !scaled {
                return Ok(());
            }

            let pod_api: Api<Pod> = Api::namespaced(client.clone(), &namespace);

            let exec_result = pod_api
                .exec(
                    &pod_name,
                    ["rcon-cli", "stop"],
                    &AttachParams::default().container(&rcon_container),
                )
                .await;

            match exec_result {
                Ok(attached) => {
                    if let Err(e) = attached
                        .join()
                        .await
                        .map_err(|e| ShutdownMinecraftServerError::Exec(Box::new(e)))
                        .with_span_trace()
                        .map_err(|e| DailyRoutineError::ShutdownMinecraftServer(sts_name.clone(), e))
                    {
                        warn!(
                            "Failed to join executed stop command on mcserver '{mcserver_name}' (pod '{}'): {}",
                            pod_name,
                            e
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to exec stop command on mcserver '{mcserver_name}' (pod '{}'): {}",
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

        result
            .inspect(|_| {
                info!("Phase 'shutdown_mcserver' for mcserver '{mcserver_name}' completed.");
            })
            .inspect_err(|e| {
                error!(
                    "Phase 'shutdown_mcserver' for mcserver '{mcserver_name}' failed: {}",
                    e
                );
            })
    }
    .instrument(span)
    .await
}

pub(crate) fn task_shutdown_mcserver(
    task_name: String,
    mcserver_name: String,
) -> TaskSpec<DailyRoutineContext, DailyRoutineError> {
    TaskSpec::new(
        task_name,
        vec!["shutdown_mcproxy".to_string()],
        move |ctx| {
            let mcserver_name = mcserver_name.clone();
            Box::pin(async move { shutdown_mcserver(ctx, mcserver_name.clone()).await })
        },
    )
}
