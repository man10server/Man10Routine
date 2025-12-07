mod raw;

use self::raw::RawConfig;
use crate::kubernetes_objects::argocd::ArgoCD;
use crate::kubernetes_objects::minecraft_chart::MinecraftChart;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub namespace: String,
    pub argocds: Arc<Vec<ArgoCD>>,
    pub mcproxy: Arc<MinecraftChart>,
    pub mcservers: Arc<Vec<MinecraftChart>>,
}

impl From<RawConfig> for Config {
    fn from(_raw: RawConfig) -> Self {
        todo!()
    }
}
