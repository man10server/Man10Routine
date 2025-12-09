pub mod error;
mod finalizer;
mod phase1;
mod phase2;
mod phase3;
mod scale_statefulset;
mod wait_until_pod_stopped;

use std::sync::Arc;

use kube::Client;
use tracing::{info, instrument};

use crate::config::Config;
use crate::scheduler::{Scheduler, Shutdown, TaskSpec};

use self::error::DailyRoutineError;
use self::phase1::task_phase1;
use self::phase2::task_phase2;
use self::phase3::task_phase3;

#[derive(Clone)]
pub(crate) struct DailyRoutineContext {
    pub(crate) config: Arc<Config>,
    pub(crate) client: Client,
}

impl DailyRoutineContext {
    pub(crate) fn new(config: Config, client: Client) -> DailyRoutineContext {
        DailyRoutineContext {
            config: Arc::new(config),
            client,
        }
    }

    #[instrument("daily_routine", skip(self))]
    pub(crate) async fn run(&self) -> Result<(), DailyRoutineError> {
        info!("Starting daily routine...");

        let shutdown = Shutdown::new();

        let tasks: Vec<TaskSpec<DailyRoutineContext, DailyRoutineError>> = vec![
            TaskSpec::new("phase1", &[], task_phase1),
            TaskSpec::new("phase2", &["phase1"], task_phase2),
            TaskSpec::new("phase3", &["phase2"], task_phase3),
        ];

        let scheduler = Scheduler::new(tasks, shutdown);
        let result = match scheduler.run(self.clone()).await {
            Ok(inner) => inner,
            Err(join_err) => Err(DailyRoutineError::TaskJoin(join_err)),
        };

        if result.is_ok() {
            info!("Daily routine completed successfully.");
        }

        self.finalizer(result).await
    }
}
