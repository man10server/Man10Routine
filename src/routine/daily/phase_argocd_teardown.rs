use super::DailyRoutineContext;
use crate::scheduler::TaskFuture;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use tokio::time::{Duration, sleep};
use tracing::{Instrument, error, info, instrument};

use crate::kubernetes_objects::minecraft_chart::MinecraftChartError;
use crate::routine::daily::DailyRoutineError;

#[instrument(name = "phase_argocd_teardown", skip(ctx))]
async fn phase_argocd_teardown(ctx: DailyRoutineContext) -> Result<(), DailyRoutineError> {
    info!("Teardown all ArgoCD applications of minecraft charts...");
    ctx.config
        .mcproxy
        .write()
        .await
        .argocd_teardown(ctx.client.clone())
        .await?;
    info!("Teardown all mcservers...");
    let mcservers: Vec<(String, _)> = ctx
        .config
        .mcservers
        .iter()
        .map(|(name, mcserver)| (name.clone(), mcserver.clone()))
        .collect();

    stream::iter(mcservers)
        .map(|(name, mcserver)| {
            let name = name.clone();
            let client = ctx.client.clone();
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

    info!("Phase 'argocd_teardown' completed. Sleeping for 10 seconds before continuing...");
    sleep(Duration::from_secs(10)).await;
    Ok(())
}

pub(crate) fn task_phase_argocd_teardown(
    ctx: DailyRoutineContext,
) -> TaskFuture<DailyRoutineError> {
    Box::pin(phase_argocd_teardown(ctx))
}
