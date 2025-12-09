use k8s_openapi::api::batch::v1::Job;
use kube::Api;
use kube::api::PostParams;
use tracing::{Instrument, error, info, instrument, trace_span};
use tracing_error::SpanTrace;

use crate::error::SpannedExt;
use crate::kubernetes_objects::MANAGEER_ROLE_NAME;
use crate::kubernetes_objects::job::CustomJob;
use crate::kubernetes_objects::minecraft_chart::WeakMinecraftChart;
use crate::routine::daily::wait_until_job_finished::wait_until_job_finished;
use crate::scheduler::TaskFuture;

use super::DailyRoutineContext;
use super::error::DailyRoutineError;

#[instrument("phase_execute_job", skip(ctx, mcserver, job))]
async fn execute_job(
    ctx: DailyRoutineContext,
    mcserver: WeakMinecraftChart,
    job_name: String,
    job: CustomJob,
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

    let result = async move {
        // Create the Job in Kubernetes
        let jobs_api: Api<Job> = Api::namespaced(client.clone(), &namespace);

        let post_params = PostParams {
            field_manager: Some(MANAGEER_ROLE_NAME.to_string()),
            ..Default::default()
        };

        let job_created = async {
            jobs_api
                .create(&post_params, &job.manifest)
                .await
                .with_span_trace()
        }
        .instrument(trace_span!("create_job"))
        .await?;

        let created_job_name = job_created.metadata.name.as_deref().unwrap_or("<unknown>");

        match wait_until_job_finished(
            client,
            &namespace,
            created_job_name,
            job.initial_wait,
            job.max_wait,
            job.max_errors,
        )
        .await
        {
            Ok(status) if status.failed == Some(0) || status.failed.is_none() => Ok(()),
            Ok(status) => Err(DailyRoutineError::CustomJobHasFailure(
                created_job_name.to_string(),
                status,
                SpanTrace::capture(),
            )),
            Err(e) => Err(e),
        }
    }
    .instrument(span)
    .await;

    match result {
        Ok(_) => {
            info!(
                "Job '{}' for mcserver '{}' executed successfully.",
                job_name, mcserver_name
            );
            Ok(())
        }
        Err(e) => {
            error!(
                "Failed to execute job '{}' for mcserver '{}': {}",
                job_name, mcserver_name, e
            );
            if job.required { Err(e) } else { Ok(()) }
        }
    }
}

pub(crate) fn task_execute_job(
    ctx: DailyRoutineContext,
    mcserver: WeakMinecraftChart,
    job_name: String,
    job: CustomJob,
) -> TaskFuture<DailyRoutineError> {
    Box::pin(execute_job(ctx, mcserver, job_name, job))
}
