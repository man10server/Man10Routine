pub mod error;
mod finalizer;
mod phase1;
mod phase2;
mod wait_until_pod_stopped;

use kube::Client;
use tracing::{info, instrument};

use crate::config::Config;

use self::error::DailyRoutineError;

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

        {
            let result = self.phase_one().await;
            if result.is_err() {
                return self.finalizer(result).await;
            }
        }

        // Sleep for 10 seconds
        info!("Phase1 completed. Sleeping for 10 seconds before continuing...");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        {
            let result = self.phase_two().await;
            if result.is_err() {
                return self.finalizer(result).await;
            }
        }

        info!("Daily routine completed successfully.");
        self.finalizer(Ok(())).await
    }
}
