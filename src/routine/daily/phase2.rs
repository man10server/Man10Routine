use super::DailyRoutineContext;
use super::error::DailyRoutineError;
use std::time::Duration;

use tracing::{info, instrument};

use crate::routine::daily::scale_statefulset::scale_statefulset_to_zero;
use crate::routine::daily::wait_until_pod_stopped::wait_until_pod_stopped;

impl DailyRoutineContext {
    #[instrument("phase2", skip(self))]
    pub(super) async fn phase_two(&mut self) -> Result<(), DailyRoutineError> {
        let proxy_sts_name = &self.config.mcproxy.read().await.name;
        info!("Stopping proxy server...");
        let scaled =
            scale_statefulset_to_zero(self.client.clone(), &self.config.namespace, proxy_sts_name)
                .await?;
        if !scaled {
            return Ok(());
        }

        wait_until_pod_stopped(
            self.client.clone(),
            &self.config.namespace,
            format!("{}-0", proxy_sts_name).as_str(),
            Duration::from_secs(60),
            Duration::from_secs(150),
            5,
        )
        .await
    }
}
