use super::DailyRoutineContext;
use crate::config::polling::PollingConfig;
use crate::scheduler::TaskFuture;

use std::time::Duration;

use tracing::{info, instrument};

use crate::kubernetes_objects::statefulset::{
    StatefulSetScaleError, scale_statefulset_to_zero, wait_until_statefulset_scaled,
};
use crate::routine::daily::error::DailyRoutineError;

#[instrument(name = "phase_relaunch_mcproxy", skip(ctx))]
async fn phase_relaunch_mcproxy(ctx: DailyRoutineContext) -> Result<(), DailyRoutineError> {
    let proxy_sts_name = &ctx.config.mcproxy.read().await.name;
    info!("Relaunching proxy server...");
    scale_statefulset_to_zero(ctx.client.clone(), &ctx.config.namespace, proxy_sts_name, 1)
        .await
        .map_err(|e| DailyRoutineError::RelaunchMinecraftServer(proxy_sts_name.to_string(), e))?;

    wait_until_statefulset_scaled(
        ctx.client.clone(),
        &ctx.config.namespace,
        proxy_sts_name,
        1,
        &PollingConfig {
            initial_wait: Duration::from_secs(10),
            poll_interval: Duration::from_secs(10),
            max_wait: Duration::from_mins(15),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| StatefulSetScaleError::StatefulSetNotScaled(proxy_sts_name.to_string(), e))
    .map_err(|e| DailyRoutineError::RelaunchMinecraftServer(proxy_sts_name.to_string(), e))?;

    info!("Phase 'relaunch_mcproxy' completed. Sleeping for 10 seconds before continuing...");
    tokio::time::sleep(Duration::from_secs(10)).await;
    Ok(())
}

pub(crate) fn task_phase_relaunch_mcproxy(
    ctx: DailyRoutineContext,
) -> TaskFuture<DailyRoutineError> {
    Box::pin(phase_relaunch_mcproxy(ctx))
}
