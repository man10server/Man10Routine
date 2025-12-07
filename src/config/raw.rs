use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct RawConfig {
    namespace: String,
    mcproxy: RawMinecraftChart,
    mcservers: Vec<RawMinecraftChart>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct RawMinecraftChart {
    /// Internal server name used charts/minecraft-v2
    name: String,

    /// ArgoCD Application Path
    ///
    /// Example: "apps/minecraft/mcserver-man10"
    argocd_path: String,

    /// Whether to use Shigen or not
    #[serde(default = "default_shigen")]
    shigen: bool,
}

const fn default_shigen() -> bool {
    false
}
