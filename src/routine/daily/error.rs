use k8s_openapi::api::batch::v1::JobStatus;
use thiserror::Error;
use tracing_error::{ExtractSpanTrace, SpanTrace};

use crate::error::SpannedErr;
use crate::kubernetes_objects::argocd::ArgoCdError;
use crate::kubernetes_objects::job::WaitJobFinishedError;
use crate::kubernetes_objects::minecraft_chart::MinecraftChartError;
use crate::kubernetes_objects::statefulset::StatefulSetScaleError;
use crate::scheduler::InvalidDagError;

#[derive(Error, Debug)]
pub enum DailyRoutineError {
    #[error("ArgoCD error: {0}")]
    ArgoCd(#[from] ArgoCdError),

    #[error("Minecraft Chart error: {0}")]
    MinecraftChart(#[from] MinecraftChartError),

    #[error("Minecraft Server {0} cannot be shutdown: {1}")]
    ShutdownMinecraftServer(String, StatefulSetScaleError),

    #[error("Job {0} cannot be finished: {1}")]
    WaitJobFinished(String, SpannedErr<WaitJobFinishedError>),

    #[error("Custom job {0} has failed with status {1:?}")]
    CustomJobHasFailure(String, JobStatus, SpanTrace),

    #[error("Kubernetes client error: {0}")]
    KubeClient(#[from] SpannedErr<kube::Error>),

    #[error("Task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("Invalid task DAG: {0}")]
    InvalidTaskDag(#[from] SpannedErr<InvalidDagError>),
}

impl ExtractSpanTrace for DailyRoutineError {
    fn span_trace(&self) -> Option<&SpanTrace> {
        match self {
            DailyRoutineError::ArgoCd(e) => e.span_trace(),
            DailyRoutineError::MinecraftChart(e) => e.span_trace(),
            DailyRoutineError::ShutdownMinecraftServer(_, e) => e.span_trace(),
            DailyRoutineError::WaitJobFinished(_, e) => e.span_trace(),
            DailyRoutineError::CustomJobHasFailure(_, _, span_trace) => Some(span_trace),
            DailyRoutineError::KubeClient(e) => e.span_trace(),
            DailyRoutineError::TaskJoin(_) => None,
            DailyRoutineError::InvalidTaskDag(e) => e.span_trace(),
        }
    }
}
