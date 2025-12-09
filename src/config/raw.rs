use std::collections::BTreeMap;

use serde::Deserialize;

#[cfg_attr(test, derive(PartialEq))]
#[derive(Deserialize, Debug, Clone)]
pub(super) struct RawConfig {
    pub(super) namespace: String,
    pub(super) mcproxy: RawMinecraftChart,
    pub(super) mcservers: BTreeMap<String, RawMinecraftChart>,
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Deserialize, Debug, Clone)]
pub(super) struct RawMinecraftChart {
    /// Internal server name used charts/minecraft-v2
    pub(super) name: Option<String>,

    /// ArgoCD Application Path
    ///
    /// Example: "apps/minecraft/mcserver-man10"
    pub(super) argocd: String,

    /// RCON Container Name
    pub(super) rcon_container: String,
}
