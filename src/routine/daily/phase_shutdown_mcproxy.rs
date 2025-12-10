use super::DailyRoutineContext;
use crate::routine::daily::MINECRAFT_SHUTDOWN_POLLING_CONFIG;
use crate::routine::daily::wait_until_statefulset_scaled::wait_until_statefulset_scaled;
use crate::scheduler::TaskFuture;

use std::time::Duration;

use tracing::{info, instrument};

use crate::routine::daily::error::{DailyRoutineError, StatefulSetScaleError};
use crate::routine::daily::scale_statefulset::scale_statefulset_to_zero;

#[instrument(name = "phase_shutdown_mcproxy", skip(ctx))]
async fn phase_shutdown_mcproxy(ctx: DailyRoutineContext) -> Result<(), DailyRoutineError> {
    let proxy_sts_name = &ctx.config.mcproxy.read().await.name;
    info!("Stopping proxy server...");
    let scaled =
        scale_statefulset_to_zero(ctx.client.clone(), &ctx.config.namespace, proxy_sts_name)
            .await
            .map_err(|e| {
                DailyRoutineError::ShutdownMinecraftServer(proxy_sts_name.to_string(), e)
            })?;
    if !scaled {
        return Ok(());
    }

    wait_until_statefulset_scaled(
        ctx.client.clone(),
        &ctx.config.namespace,
        proxy_sts_name,
        0,
        MINECRAFT_SHUTDOWN_POLLING_CONFIG,
    )
    .await
    .map_err(|e| StatefulSetScaleError::StatefulSetNotScaled(proxy_sts_name.to_string(), e))
    .map_err(|e| DailyRoutineError::ShutdownMinecraftServer(proxy_sts_name.to_string(), e))?;

    info!("Phase 'shutdown_mcproxy' completed. Sleeping for 10 seconds before continuing...");
    tokio::time::sleep(Duration::from_secs(10)).await;
    Ok(())
}

pub(crate) fn task_phase_shutdown_mcproxy(
    ctx: DailyRoutineContext,
) -> TaskFuture<DailyRoutineError> {
    Box::pin(phase_shutdown_mcproxy(ctx))
}
