pub mod error;
mod finalizer;
mod phase_argocd_teardown;
mod phase_execute_job;
mod phase_relaunch_mcproxy;
mod phase_relaunch_mcserver;
mod phase_shutdown_mcproxy;
mod phase_shutdown_mcservers;

use std::iter;
use std::sync::Arc;

use futures::{StreamExt, future, stream};
use kube::Client;
use tracing::{info, instrument};

use crate::config::Config;
use crate::scheduler::{Scheduler, Shutdown, TaskSpec};

use self::error::DailyRoutineError;
use self::phase_argocd_teardown::task_phase_argocd_teardown;
use self::phase_execute_job::task_execute_job;
use self::phase_relaunch_mcproxy::task_phase_relaunch_mcproxy;
use self::phase_relaunch_mcserver::task_relaunch_mcserver;
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
        let tasks = build_daily_tasks(self).await;
        let scheduler = Scheduler::from_tasks(tasks, shutdown)?;
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

async fn build_daily_tasks(
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
        .iter()
        .map(|(name, mcserver)| {
            task_shutdown_mcserver(
                format!("shutdown_mcserver/{}", name),
                Arc::downgrade(mcserver),
            )
        })
        .for_each(|task| tasks.push(task));

    stream::iter(ctx.config.mcservers.iter())
        .then(async |(name, mcserver)| {
            let weak_mcserver = Arc::downgrade(mcserver);
            mcserver
                .read()
                .await
                .jobs_after_snapshot
                .clone()
                .into_iter()
                .map(move |(job_name, job)| (name.clone(), weak_mcserver.clone(), job_name, job))
        })
        .flat_map_unordered(None, stream::iter)
        .map(|(mcserver_name, mcserver, job_name, job)| {
            TaskSpec::new(
                format!("execute_job/after_snapshot/{}/{}", mcserver_name, job_name),
                job.dependencies
                    .iter()
                    .map(|d| format!("execute_job/after_snapshot/{}/{}", mcserver_name, d))
                    .chain(iter::once(format!("shutdown_mcserver/{}", mcserver_name)))
                    .collect::<Vec<_>>(),
                move |ctx| task_execute_job(ctx, mcserver, job_name, job),
            )
        })
        .for_each(|task| {
            tasks.push(task);
            future::ready(())
        })
        .await;

    stream::iter(ctx.config.mcservers.iter())
        .then(async |(name, mcserver)| {
            let weak_mcserver = Arc::downgrade(mcserver);
            let deps: Vec<String> = mcserver
                .read()
                .await
                .jobs_after_snapshot
                .clone()
                .into_keys()
                .map(|d| format!("execute_job/after_snapshot/{}/{}", name, d))
                .chain(iter::once(format!("shutdown_mcserver/{}", name)))
                .collect();
            (name.clone(), weak_mcserver, deps)
        })
        .map(|(mcserver_name, mcserver, deps)| {
            TaskSpec::new(
                format!("relaunch_mcserver/{}", mcserver_name),
                deps,
                move |ctx| task_relaunch_mcserver(ctx, mcserver),
            )
        })
        .for_each(|task| {
            tasks.push(task);
            future::ready(())
        })
        .await;

    tasks.push(TaskSpec::new(
        "relaunch_mcproxy",
        stream::iter(ctx.config.mcservers.iter())
            .filter(|(_, mcserver)| async { mcserver.read().await.required_to_start })
            .map(|(name, _)| format!("relaunch_mcserver/{}", name))
            .chain(stream::once(future::ready("shutdown_mcproxy".to_string())))
            .collect::<Vec<_>>()
            .await,
        move |ctx| task_phase_relaunch_mcproxy(ctx),
    ));

    tasks
}
