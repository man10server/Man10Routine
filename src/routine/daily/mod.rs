pub mod error;
mod finalizer;
mod phase_argocd_teardown;
mod phase_shutdown_mcproxy;
mod phase_shutdown_mcservers;
mod scale_statefulset;
mod wait_until_pod_stopped;

use std::sync::Arc;

use kube::Client;
use tracing::{info, instrument};

use crate::config::Config;
use crate::scheduler::{Scheduler, Shutdown, TaskSpec};

use self::error::DailyRoutineError;
use self::phase_argocd_teardown::task_phase_argocd_teardown;
use self::phase_shutdown_mcproxy::task_phase_shutdown_mcproxy;
use self::phase_shutdown_mcservers::task_shutdown_mcserver;

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
        let tasks = build_daily_tasks(self);
        let scheduler = Scheduler::from_tasks(tasks, shutdown);
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

fn build_daily_tasks(
    ctx: &DailyRoutineContext,
) -> Vec<TaskSpec<DailyRoutineContext, DailyRoutineError>> {
    let mut tasks = Vec::new();

    tasks.push(TaskSpec::new(
        "argocd_teardown",
        Vec::<String>::new(),
        task_phase_argocd_teardown,
    ));

    tasks.push(TaskSpec::new(
        "shutdown_mcproxy",
        vec!["argocd_teardown".to_string()],
        task_phase_shutdown_mcproxy,
    ));

    ctx.config
        .mcservers
        .keys()
        .map(|name| task_shutdown_mcserver(format!("shutdown_mcserver/{}", name), name.clone()))
        .for_each(|task| tasks.push(task));

    tasks
}
