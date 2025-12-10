use crate::kubernetes_objects::argocd::tearing::TearingArgoCd;
use kube::Client;
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::Level;
use tracing_error::{ExtractSpanTrace, SpanTrace};

use super::argocd::tearing::TearingArgoCdGuard;
use super::argocd::{ArgoCdError, WeakArgoCd};
use super::custom_job::CustomJob;

pub(crate) type SharedMinecraftChart = Arc<RwLock<MinecraftChart>>;
pub(crate) type WeakMinecraftChart = Weak<RwLock<MinecraftChart>>;

#[derive(Debug)]
pub(crate) struct MinecraftChart {
    /// Internal server name used charts/minecraft-v2
    pub(crate) name: String,

    /// ArgoCD Application
    pub(crate) argocd: WeakArgoCd,

    /// RCON Container name
    pub(crate) rcon_container: String,

    /// Custom jobs that have been created after snapshot of the volumes were taken
    pub(crate) jobs_after_snapshot: BTreeMap<String, CustomJob>,

    argocd_tear: Option<Result<TearingArgoCdGuard, ArgoCdError>>,
}

#[derive(Error, Debug)]
pub enum MinecraftChartError {
    #[error("ArgoCD error: {0}")]
    ArgocdError(#[from] ArgoCdError),
}

impl ExtractSpanTrace for MinecraftChartError {
    fn span_trace(&self) -> Option<&SpanTrace> {
        match self {
            MinecraftChartError::ArgocdError(e) => e.span_trace(),
        }
    }
}

impl MinecraftChart {
    pub(crate) fn new(
        name: String,
        argocd: WeakArgoCd,
        rcon_container: String,
        jobs_after_snapshot: BTreeMap<String, CustomJob>,
    ) -> SharedMinecraftChart {
        Arc::new(RwLock::new(MinecraftChart {
            name,
            argocd,
            rcon_container,
            jobs_after_snapshot,
            argocd_tear: None,
        }))
    }

    #[tracing::instrument(
        "minecraft_chart/argocd_teardown",
        level = Level::TRACE,
        skip(self, client),
        fields(minecraft_chart_name = %self.name)
    )]
    pub(crate) async fn argocd_teardown(
        &mut self,
        client: Client,
    ) -> Result<(), MinecraftChartError> {
        match self.argocd_tear {
            Some(Ok(_)) => Ok(()),
            Some(Err(ref e)) => Err(MinecraftChartError::ArgocdError(e.clone())),
            None => {
                match self.argocd.upgrade().unwrap_or_else(|| panic!(
                    "MinecraftChart '{}' cannot perform ArgoCD teardown because its ArgoCD object has been dropped",
                    self.name
                )).tear(client).await {
                    Ok(tear) => {
                        self.argocd_tear = Some(Ok(tear));
                        Ok(())
                    }
                    Err(e) => {
                        self.argocd_tear = Some(Err(e.clone()));
                        Err(MinecraftChartError::ArgocdError(e))
                    }
                }
            }
        }
    }

    #[tracing::instrument(
        "minecraft_chart/release",
        level = Level::TRACE,
        skip(self),
        fields(minecraft_chart_name = %self.name)
    )]
    pub(crate) async fn release(&mut self) -> Result<(), ArgoCdError> {
        if let Some(Ok(tear)) = self.argocd_tear.take() {
            tear.close().await
        } else {
            Ok(())
        }
    }
}

impl Drop for MinecraftChart {
    fn drop(&mut self) {
        if self.argocd_tear.take().is_some() {
            panic!(
                "MinecraftChart '{}' is being dropped before its ArgoCD teardown guard was closed",
                self.name,
            );
        }
    }
}
