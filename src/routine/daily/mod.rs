pub mod error;
mod finalizer;
mod phase1;
mod phase2;
mod phase3;
mod scale_statefulset;
mod shutdown;
mod wait_until_pod_stopped;

use kube::Client;
use tokio::time::Duration;
use tracing::{info, instrument};

use crate::config::Config;

use self::error::DailyRoutineError;
use self::shutdown::Shutdown;

pub(crate) struct DailyRoutineContext {
    config: Config,
    client: Client,
}

impl DailyRoutineContext {
    pub(crate) fn new(config: Config, client: Client) -> DailyRoutineContext {
        DailyRoutineContext { config, client }
    }

    #[instrument("daily_routine", skip(self))]
    pub(crate) async fn run(&mut self) -> Result<(), DailyRoutineError> {
        info!("Starting daily routine...");

        let mut shutdown = Shutdown::new();

        {
            let result = shutdown
                .run_phase_with_shutdown("phase1", self.phase_one())
                .await;
            if result.is_err() {
                return self.finalizer(result).await;
            }

            if shutdown.requested() {
                info!("Shutdown requested. Running finalizer before exit.");
                return self.finalizer(result).await;
            }
        }

        // Sleep for 10 seconds
        info!("Phase1 completed. Sleeping for 10 seconds before continuing...");
        if shutdown.sleep_or_shutdown(Duration::from_secs(10)).await {
            info!("Shutdown requested. Running finalizer before exit.");
            return self.finalizer(Ok(())).await;
        }

        {
            let result = shutdown
                .run_phase_with_shutdown("phase2", self.phase_two())
                .await;
            if result.is_err() {
                return self.finalizer(result).await;
            }

            if shutdown.requested() {
                info!("Shutdown requested. Running finalizer before exit.");
                return self.finalizer(result).await;
            }
        }

        // Sleep for 10 seconds
        info!("Phase2 completed. Sleeping for 10 seconds before continuing...");
        if shutdown.sleep_or_shutdown(Duration::from_secs(10)).await {
            info!("Shutdown requested. Running finalizer before exit.");
            return self.finalizer(Ok(())).await;
        }

        {
            let result = shutdown
                .run_phase_with_shutdown("phase3", self.phase_three())
                .await;
            if result.is_err() {
                return self.finalizer(result).await;
            }

            if shutdown.requested() {
                info!("Shutdown requested. Running finalizer before exit.");
                return self.finalizer(result).await;
            }
        }

        // Sleep for 30 seconds
        info!("Phase3 completed. Sleeping for 30 seconds for the databases to stabilize...");
        if shutdown.sleep_or_shutdown(Duration::from_secs(30)).await {
            info!("Shutdown requested. Running finalizer before exit.");
            return self.finalizer(Ok(())).await;
        }

        info!("Daily routine completed successfully.");
        self.finalizer(Ok(())).await
    }
}
