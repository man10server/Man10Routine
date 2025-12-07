use std::sync::Arc;
use tokio::sync::RwLock;

use super::argocd::WeakArgoCd;

pub(crate) type SharedMinecraftChart = Arc<RwLock<MinecraftChart>>;

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct MinecraftChart {
    /// Internal server name used charts/minecraft-v2
    pub name: String,

    /// ArgoCD Application
    pub argocd: WeakArgoCd,

    /// Whether to use Shigen or not
    pub shigen: bool,
}
