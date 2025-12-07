use crate::kubernetes_objects::argocd::ArgoCD;
use std::sync::Weak;
use tokio::sync::RwLock;
#[derive(Debug)]
pub(crate) struct MinecraftChart {
    /// Internal server name used charts/minecraft-v2
    pub name: String,

    /// ArgoCD Application
    pub argocd: RwLock<Weak<ArgoCD>>,
}
