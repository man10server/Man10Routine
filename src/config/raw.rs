use std::collections::BTreeMap;

use super::Config;
use crate::kubernetes_objects::argocd::SharedArgoCd;
use crate::kubernetes_objects::minecraft_chart::MinecraftChart;
use serde::Deserialize;
use thiserror::Error;

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

#[derive(Error, Debug)]
pub enum ConfigParseError {
    #[error(
        "Parent mismatch for ArgoCD app '{name}': first defined parent '{first_parent}', but afterwards defined parent '{second_parent}'"
    )]
    ArgoCdParentMismatch {
        name: String,
        first_parent: String,
        second_parent: String,
    },

    #[error("ArgoCD app '{name}' has multiple minecraft charts assigned")]
    ArgoCdHasMultipleCharts { name: String },

    #[error("mcproxy must have a name defined")]
    McproxyNameMissing,

    #[error("Key '{name}' must not contain '/' characters")]
    KeyIncludesSlash { name: String },
}

impl TryFrom<RawConfig> for Config {
    type Error = ConfigParseError;
    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        let mut argocds: BTreeMap<String, SharedArgoCd> = BTreeMap::new();

        for name in raw.mcservers.keys() {
            if name.contains('/') {
                return Err(ConfigParseError::KeyIncludesSlash { name: name.clone() });
            }
        }

        let namespace = raw.namespace;
        let mcproxy_argocd = Self::build_argocd_hierarchy(&mut argocds, &raw.mcproxy.argocd)?;
        let mcproxy = MinecraftChart::new(
            raw.mcproxy
                .name
                .ok_or(ConfigParseError::McproxyNameMissing)?,
            mcproxy_argocd,
            raw.mcproxy.rcon_container,
        );
        let mcservers = raw
            .mcservers
            .into_iter()
            .map(|(name, server)| {
                let server_argocd = Self::build_argocd_hierarchy(&mut argocds, &server.argocd)?;
                let mc_chart = MinecraftChart::new(
                    server.name.unwrap_or_else(|| name.clone()),
                    server_argocd,
                    server.rcon_container,
                );
                Ok((name, mc_chart))
            })
            .collect::<Result<BTreeMap<_, _>, ConfigParseError>>()?;

        Ok(Config {
            namespace,
            argocds,
            mcproxy,
            mcservers,
        })
    }
}
