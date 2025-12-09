use super::DailyRoutineContext;
use crate::scheduler::TaskFuture;

use std::time::Duration;

use tracing::{info, instrument};

use crate::routine::daily::error::DailyRoutineError;
use crate::routine::daily::scale_statefulset::scale_statefulset_to_zero;
use crate::routine::daily::wait_until_pod_stopped::wait_until_pod_stopped;

#[instrument(name = "phase_shutdown_mcproxy", skip(ctx))]
async fn phase_shutdown_mcproxy(ctx: DailyRoutineContext) -> Result<(), DailyRoutineError> {
    let proxy_sts_name = &ctx.config.mcproxy.read().await.name;
    info!("Stopping proxy server...");
    let scaled =
        scale_statefulset_to_zero(ctx.client.clone(), &ctx.config.namespace, proxy_sts_name)
            .await?;
    if !scaled {
        return Ok(());
    }

    wait_until_pod_stopped(
        ctx.client.clone(),
        &ctx.config.namespace,
        format!("{}-0", proxy_sts_name).as_str(),
        Duration::from_secs(60),
        Duration::from_secs(150),
        5,
    )
    .await?;

    info!("Phase 'shutdown_mcproxy' completed. Sleeping for 10 seconds before continuing...");
    tokio::time::sleep(Duration::from_secs(10)).await;
    Ok(())
}

pub(crate) fn task_phase_shutdown_mcproxy(
    ctx: DailyRoutineContext,
) -> TaskFuture<DailyRoutineError> {
    Box::pin(phase_shutdown_mcproxy(ctx))
}
