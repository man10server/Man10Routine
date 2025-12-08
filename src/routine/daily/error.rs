use thiserror::Error;
use tracing::error;
use tracing_error::TracedError;
use tracing_error::{ExtractSpanTrace, SpanTrace};

use crate::kubernetes_objects::argocd::ArgoCdError;
use crate::kubernetes_objects::minecraft_chart::MinecraftChartError;

#[derive(Error, Debug)]
pub enum DailyRoutineError {
    #[error("ArgoCD error: {0}")]
    ArgoCd(#[from] ArgoCdError),

    #[error("Minecraft Chart error: {0}")]
    MinecraftChart(#[from] MinecraftChartError),

    #[error("Minecraft Server {0} cannot be shutdown: {1}")]
    ShutdownMinecraftServer(String, TracedError<ShutdownMinecraftServerError>),

    #[error("Kubernetes client error: {0}")]
    KubeClient(#[from] TracedError<kube::Error>),
}

#[derive(Error, Debug)]
pub enum ShutdownMinecraftServerError {
    #[error("Kubernetes client error: {0}")]
    KubeClient(kube::Error),

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
            DailyRoutineError::ShutdownMinecraftServer(_, e) => {
                (e as &(dyn std::error::Error + 'static)).span_trace()
            }
            DailyRoutineError::KubeClient(e) => {
                (e as &(dyn std::error::Error + 'static)).span_trace()
            }
        }
    }
}
