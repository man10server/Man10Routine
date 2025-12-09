use super::DailyRoutineContext;

use tracing::error;
use tracing::{info, instrument};
use tracing_error::ExtractSpanTrace;

use crate::routine::daily::DailyRoutineError;

impl DailyRoutineContext {
    #[instrument("finalizer", skip(self, result))]
    pub(super) async fn finalizer(
        &self,
        result: Result<(), DailyRoutineError>,
    ) -> Result<(), DailyRoutineError> {
        info!("Tearup all ArgoCD applications of minecraft charts...");
        if let Err(e) = self.config.mcproxy.write().await.release().await {
            error!("Failed to release mcproxy: {}", e);
            if let Some(span_trace) = e.span_trace() {
                eprintln!("\n{}\n", color_spantrace::colorize(span_trace));
            }
        }
        for (name, mcserver) in self.config.mcservers.iter() {
            if let Err(e) = mcserver.write().await.release().await {
                error!("Failed to release mcserver '{name}': {}", e);
                if let Some(span_trace) = e.span_trace() {
                    eprintln!("\n{}\n", color_spantrace::colorize(span_trace));
                }
            }
        }

        result
    }
}
