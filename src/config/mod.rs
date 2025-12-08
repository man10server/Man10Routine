#![allow(unused)]
pub(crate) mod raw;

use std::collections::BTreeMap;
use std::iter;
use std::path::PathBuf;
use std::sync::Arc;

use self::raw::RawConfig;
use crate::kubernetes_objects::argocd::{ArgoCd, SharedArgoCd, WeakArgoCd};
use crate::kubernetes_objects::minecraft_chart::{MinecraftChart, SharedMinecraftChart};
use thiserror::Error;
use tokio::fs::read_to_string;
use tokio::io;

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub(crate) namespace: String,
    pub(crate) argocds: BTreeMap<String, SharedArgoCd>,
    pub(crate) mcproxy: SharedMinecraftChart,
    pub(crate) mcservers: BTreeMap<String, SharedMinecraftChart>,
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
}

#[derive(Error, Debug)]
pub enum ConfigLoadError {
    #[error("Config file {0} is not valid: {1}")]
    ContentInvalid(PathBuf, ConfigParseError),

    #[error("Config file {0} is not serializable: {1}")]
    ContentUnserializable(PathBuf, toml::de::Error),

    #[error("Config file {0} cannot be read: {1}")]
    FailedToRead(PathBuf, io::Error),
}

impl Config {
    pub async fn new_from_file(path: &PathBuf) -> Result<Self, ConfigLoadError> {
        let source_string = read_to_string(path)
            .await
            .map_err(|e| ConfigLoadError::FailedToRead(path.clone(), e))?;

        let raw_config: RawConfig = toml::from_str(&source_string)
            .map_err(|e| ConfigLoadError::ContentUnserializable(path.clone(), e))?;

        let config = Self::try_from(raw_config)
            .map_err(|e| ConfigLoadError::ContentInvalid(path.clone(), e))?;

        Ok(config)
    }

    fn get_or_insert_argocd_apps_of_apps(
        argocds: &mut BTreeMap<String, SharedArgoCd>,
        parent_name: Option<&str>,
        name: &str,
    ) -> Result<WeakArgoCd, ConfigParseError> {
        let app_of_apps = match argocds.get(name) {
            Some(app) => {
                let app_read = app
                    .try_read()
                    .expect("initializing Config failed: ArgoCD RwLock poisoned");
                let parent = app_read.parent.as_ref().map(|p| {
                    p.upgrade()
                        .expect("initializing Config failed: Parent ArgoCD object has been dropped")
                });
                let read_parent = parent.as_ref().map(|parent| {
                    parent
                        .try_read()
                        .expect("initializing Config failed: ArgoCD RwLock poisoned")
                });

                let first_parent = read_parent.as_ref().map(|p| p.name.as_str());

                if first_parent == parent_name {
                    Ok(app.clone())
                } else {
                    Err(ConfigParseError::ArgoCdParentMismatch {
                        name: name.to_string(),
                        first_parent: first_parent.unwrap_or("None").to_string(),
                        second_parent: parent_name.unwrap_or("None").to_string(),
                    })
                }
            }
            None => {
                let parent = match parent_name {
                    Some(p_name) => Some(Arc::downgrade(
                        &argocds
                            .get(p_name)
                            .expect("initializing Config failed: Parent ArgoCD app should exist in advance")
                            .clone(),
                    )),
                    None => None,
                };
                let app = ArgoCd::new_app_of_apps(name.to_string(), parent);
                argocds.insert(name.to_string(), app.clone());
                Ok(app)
            }
        };
        Ok(Arc::downgrade(&app_of_apps?))
    }

    fn insert_argocd_application(
        argocds: &mut BTreeMap<String, SharedArgoCd>,
        parent_name: Option<&str>,
        name: &str,
    ) -> Result<WeakArgoCd, ConfigParseError> {
        let None = argocds.get(name) else {
            return Err(ConfigParseError::ArgoCdHasMultipleCharts {
                name: name.to_string(),
            });
        };

        let parent = match parent_name {
            Some(p_name) => Some(Arc::downgrade(
                &argocds
                    .get(p_name)
                    .expect("Parent ArgoCD app should exist")
                    .clone(),
            )),
            None => None,
        };
        let app = ArgoCd::new_application(name.to_string(), parent);
        argocds.insert(name.to_string(), app.clone());
        Ok(Arc::downgrade(&app))
    }

    fn build_argocd_hierarchy(
        argocds: &mut BTreeMap<String, SharedArgoCd>,
        path: &str,
    ) -> Result<WeakArgoCd, ConfigParseError> {
        let path: Vec<&str> = path.split('/').collect();
        let (&last, rest) = path.split_last().expect("Path should not be empty");

        for (paren_name, name) in iter::once(None)
            .chain(rest.iter().map(Some))
            .zip(rest.iter())
        {
            Self::get_or_insert_argocd_apps_of_apps(argocds, paren_name.copied(), name)?;
        }

        Self::insert_argocd_application(argocds, rest.iter().last().copied(), last)
    }
}

impl TryFrom<RawConfig> for Config {
    type Error = ConfigParseError;
    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        let mut argocds: BTreeMap<String, SharedArgoCd> = BTreeMap::new();

        let namespace = raw.namespace;
        let mcproxy_argocd = Self::build_argocd_hierarchy(&mut argocds, &raw.mcproxy.argocd)?;
        let mcproxy = MinecraftChart::new(
            raw.mcproxy
                .name
                .ok_or(ConfigParseError::McproxyNameMissing)?,
            mcproxy_argocd,
            raw.mcproxy.shigen,
        );
        let mcservers = raw
            .mcservers
            .into_iter()
            .map(|(name, server)| {
                let server_argocd = Self::build_argocd_hierarchy(&mut argocds, &server.argocd)?;
                let mc_chart = MinecraftChart::new(
                    server.name.unwrap_or_else(|| name.clone()),
                    server_argocd,
                    server.shigen,
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

#[cfg(test)]
mod tests {

    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_config_parse() {
        let raw = RawConfig {
            namespace: "default".to_string(),
            mcproxy: raw::RawMinecraftChart {
                name: Some("mcproxy".to_string()),
                argocd: "apps/minecraft/mcproxy".to_string(),
                shigen: false,
            },
            mcservers: BTreeMap::from([
                (
                    "server1".to_string(),
                    raw::RawMinecraftChart {
                        name: Some("server1_customname".to_string()),
                        argocd: "apps/minecraft/servers/server1".to_string(),
                        shigen: false,
                    },
                ),
                (
                    "server2".to_string(),
                    raw::RawMinecraftChart {
                        name: None,
                        argocd: "apps/minecraft/servers/server2".to_string(),
                        shigen: true,
                    },
                ),
            ]),
        };

        let config = match Config::try_from(raw) {
            Ok(cfg) => cfg,
            Err(e) => panic!("Config parse failed: {}", e),
        };
        assert_eq!(config.namespace, "default");
        assert_eq!(config.argocds.len(), 6);
        {
            let server_1 = config.mcservers.get("server1").unwrap().try_read().unwrap();
            assert_eq!(server_1.name, "server1_customname");
            assert!(!server_1.shigen);
        }
        {
            let server_2 = config.mcservers.get("server2").unwrap().try_read().unwrap();
            assert_eq!(server_2.name, "server2");
            assert!(server_2.shigen);
        }
        {
            let server_1 = config.mcservers.get("server1").unwrap().try_read().unwrap();
            let server_1_argocd = server_1.argocd.upgrade().unwrap();
            assert_eq!(
                server_1_argocd.try_read().unwrap().path,
                vec!["apps", "minecraft", "servers", "server1"]
            );
            assert_eq!(server_1_argocd.try_read().unwrap().name, "server1");
        }
    }
}
