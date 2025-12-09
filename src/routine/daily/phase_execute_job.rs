use k8s_openapi::api::batch::v1::Job;
use tracing::{instrument, trace_span};

use crate::kubernetes_objects::minecraft_chart::WeakMinecraftChart;
use crate::scheduler::TaskFuture;

use super::DailyRoutineContext;
use super::error::DailyRoutineError;

#[allow(unused)]
#[instrument("phase_execute_job", skip(ctx, mcserver, job))]
async fn execute_job(
    ctx: DailyRoutineContext,
    mcserver: WeakMinecraftChart,
    job_name: String,
    job: Job,
) -> Result<(), DailyRoutineError> {
    let client = ctx.client.clone();
    let namespace = ctx.config.namespace.clone();

    let mcserver = mcserver.upgrade().expect("MinecraftChart has been dropped");
    let read = &mcserver.read().await;

    let mcserver_name = &read.name;

    let span = trace_span!(
        "execute_job",
        kubernetes_namespace = %namespace,
        mcserver_name = %mcserver_name,
        job_name = %job_name
    );
    todo!()
}

pub(crate) fn task_execute_job(
    ctx: DailyRoutineContext,
    mcserver: WeakMinecraftChart,
    job_name: String,
    job: Job,
) -> TaskFuture<DailyRoutineError> {
    Box::pin(execute_job(ctx, mcserver, job_name, job))
}
