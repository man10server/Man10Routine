use thiserror::Error;
use tracing_error::{ExtractSpanTrace, SpanTrace};

use crate::error::SpannedErr;
use crate::kubernetes_objects::argocd::ArgoCdError;
use crate::kubernetes_objects::minecraft_chart::MinecraftChartError;
use crate::scheduler::InvalidDagError;

#[derive(Error, Debug)]
pub enum DailyRoutineError {
    #[error("ArgoCD error: {0}")]
    ArgoCd(#[from] ArgoCdError),

    #[error("Minecraft Chart error: {0}")]
    MinecraftChart(#[from] MinecraftChartError),

    #[error("Minecraft Server {0} cannot be shutdown: {1}")]
    ShutdownMinecraftServer(String, SpannedErr<ShutdownMinecraftServerError>),

    #[error("Kubernetes client error: {0}")]
    KubeClient(#[from] SpannedErr<kube::Error>),

    #[error("Task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("Invalid task DAG: {0}")]
    InvalidTaskDag(#[from] SpannedErr<InvalidDagError>),
}

#[derive(Error, Debug)]
pub enum ShutdownMinecraftServerError {
    #[error("Kubernetes client error: {0}")]
    KubeClient(kube::Error),

    #[error("Exec command error: {0}")]
    Exec(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Failed to scale Statefulset: the statefulset has no 'replicas' field")]
    StatefulSetNoReplicas,

    #[error("Pod did not shutdown within {0} seconds timeout")]
    PodShutdownCheckTimeout(u64),
}

impl ExtractSpanTrace for DailyRoutineError {
    fn span_trace(&self) -> Option<&SpanTrace> {
        match self {
            DailyRoutineError::ArgoCd(e) => e.span_trace(),
            DailyRoutineError::MinecraftChart(e) => e.span_trace(),
            DailyRoutineError::ShutdownMinecraftServer(_, e) => e.span_trace(),
            DailyRoutineError::KubeClient(e) => e.span_trace(),
            DailyRoutineError::TaskJoin(_) => None,
            DailyRoutineError::InvalidTaskDag(e) => e.span_trace(),
        }
    }
}
