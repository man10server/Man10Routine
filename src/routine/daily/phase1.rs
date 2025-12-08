use super::DailyRoutineContext;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use tracing::{Instrument, error};
use tracing::{info, instrument};

use crate::kubernetes_objects::minecraft_chart::MinecraftChartError;
use crate::routine::daily::DailyRoutineError;

impl DailyRoutineContext {
    #[instrument("phase1", skip(self))]
    pub(super) async fn phase_one(&mut self) -> Result<(), DailyRoutineError> {
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
}
