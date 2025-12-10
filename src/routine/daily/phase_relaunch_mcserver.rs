use std::time::Duration;

use tracing::{error, info, instrument, trace_span, Instrument};

use crate::config::polling::PollingConfig;
use crate::kubernetes_objects::minecraft_chart::WeakMinecraftChart;
use crate::kubernetes_objects::statefulset::{scale_statefulset_to_zero, wait_until_statefulset_scaled, StatefulSetScaleError};
use crate::scheduler::TaskFuture;

use super::error::DailyRoutineError;
use super::DailyRoutineContext;


#[instrument("phase_relaunch_mcserver", skip_all)]
async fn relaunch_mcserver(
    ctx: DailyRoutineContext,
    mcserver: WeakMinecraftChart,
) -> Result<(), DailyRoutineError> {
    let client = ctx.client.clone();
    let namespace = ctx.config.namespace.clone();

    let mcserver = mcserver.upgrade().expect("MinecraftChart has been dropped");
    let read = mcserver.read().await;

    let (mcserver_name, sts_name, rcon_container) =
        { (&read.name, &read.name, &read.rcon_container) };


    let span = trace_span!(
        "relaunch_mcserver",
        kubernetes_namespace = %namespace,
        statefulset_name = %sts_name,
        mcserver_name = %mcserver_name,
        rcon_container = %rcon_container,
    );

    async move {
        let result: Result<(), DailyRoutineError> = async {
            scale_statefulset_to_zero(client.clone(), &namespace, sts_name, 1)
                .await
                .map_err(|e| {
                    DailyRoutineError::RelaunchMinecraftServer(sts_name.clone(), e)
                })?;

            wait_until_statefulset_scaled(
                client.clone(),
                &namespace,
                sts_name,
                1,
                &PollingConfig {
                    initial_wait: Duration::from_secs(10),
                    poll_interval: Duration::from_secs(10),
                    max_wait: Duration::from_mins(15),
                    ..Default::default()
                },
            )
            .await
                .map_err(|e| {
                    StatefulSetScaleError::StatefulSetNotScaled(sts_name.clone(), e)
                })
                .map_err(|e| DailyRoutineError::ShutdownMinecraftServer(sts_name.clone(), e))?;

            Ok(())
        }
        .await;

        result
            .inspect(|_| {
                info!("Phase 'relaunch_mcserver' for mcserver '{mcserver_name}' completed.");
            })
            .inspect_err(|e| {
                error!(
                    "Phase 'relaunch_mcserver' for mcserver '{mcserver_name}' failed: {}",
                    e
                );
            })?;

        info!("Sleeping for 3 minutes to allow the server to fully start...");
        tokio::time::sleep(Duration::from_mins(3)).await;
        Ok(())
    }
    .instrument(span)
    .await
}
    
pub(crate) fn task_relaunch_mcserver(
    ctx: DailyRoutineContext,
    mcserver: WeakMinecraftChart,
) -> TaskFuture<DailyRoutineError> {
    Box::pin(relaunch_mcserver(ctx, mcserver))
}
