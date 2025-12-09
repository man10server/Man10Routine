use super::DailyRoutineContext;
use crate::routine::daily::scheduler::TaskFuture;

use std::time::Duration;

use tracing::{info, instrument};

use crate::routine::daily::scale_statefulset::scale_statefulset_to_zero;
use crate::routine::daily::wait_until_pod_stopped::wait_until_pod_stopped;

#[instrument(name = "phase2", skip(ctx))]
async fn phase2(ctx: DailyRoutineContext) -> Result<(), super::error::DailyRoutineError> {
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

    info!("Phase2 completed. Sleeping for 10 seconds before continuing...");
    tokio::time::sleep(Duration::from_secs(10)).await;
    Ok(())
}

pub(crate) fn task_phase2(ctx: DailyRoutineContext) -> TaskFuture {
    Box::pin(phase2(ctx))
}
