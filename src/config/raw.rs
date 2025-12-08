use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub(super) struct RawConfig {
    pub(super) namespace: String,
    pub(super) mcproxy: RawMinecraftChart,
    pub(super) mcservers: BTreeMap<String, RawMinecraftChart>,
}

#[derive(Deserialize, Debug, Clone)]
pub(super) struct RawMinecraftChart {
    /// Internal server name used charts/minecraft-v2
    pub(super) name: Option<String>,

    /// ArgoCD Application Path
    ///
    /// Example: "apps/minecraft/mcserver-man10"
    pub(super) argocd: String,

    /// Whether to use Shigen or not
    #[serde(default = "default_shigen")]
    pub(super) shigen: bool,

    /// RCON Container Name
    pub(super) rcon_container: String,
}

const fn default_shigen() -> bool {
    false
}
