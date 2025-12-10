pub mod polling;
pub(crate) mod raw;

use std::collections::BTreeMap;
use std::iter;
use std::path::PathBuf;
use std::sync::Arc;

pub use self::raw::ConfigParseError;
use self::raw::RawConfig;
use crate::kubernetes_objects::argocd::{ArgoCd, SharedArgoCd, WeakArgoCd};
use crate::kubernetes_objects::minecraft_chart::SharedMinecraftChart;
use thiserror::Error;
use tokio::fs::read_to_string;
use tokio::io;

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub(crate) namespace: String,
    #[allow(dead_code)]
    argocds: BTreeMap<String, SharedArgoCd>,
    pub(crate) mcproxy: SharedMinecraftChart,
    pub(crate) mcservers: BTreeMap<String, SharedMinecraftChart>,
}

#[derive(Error, Debug)]
pub enum ConfigLoadError {
    #[error("Config file {0} is not valid: {1}")]
    ContentInvalid(PathBuf, ConfigParseError),

    #[error("Config file {0} is not serializable: {1}")]
    ContentUnserializable(PathBuf, serde_yaml::Error),

    #[error("Config file {0} cannot be read: {1}")]
    FailedToRead(PathBuf, io::Error),
}

impl Config {
    pub async fn new_from_file(path: &PathBuf) -> Result<Self, ConfigLoadError> {
        let source_string = read_to_string(path)
            .await
            .map_err(|e| ConfigLoadError::FailedToRead(path.clone(), e))?;

        let raw_config: RawConfig = serde_yaml::from_str(&source_string)
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
                rcon_container: "mcproxy".to_string(),
                jobs_after_snapshot: BTreeMap::new(),
                required_to_start: None,
            },
            mcservers: BTreeMap::from([
                (
                    "server1".to_string(),
                    raw::RawMinecraftChart {
                        name: Some("server1_customname".to_string()),
                        argocd: "apps/minecraft/servers/server1".to_string(),
                        rcon_container: "server1".to_string(),
                        jobs_after_snapshot: BTreeMap::new(),
                        required_to_start: None,
                    },
                ),
                (
                    "server2".to_string(),
                    raw::RawMinecraftChart {
                        name: None,
                        argocd: "apps/minecraft/servers/server2".to_string(),
                        rcon_container: "server2".to_string(),
                        jobs_after_snapshot: BTreeMap::new(),
                        required_to_start: Some(false),
                    },
                ),
            ]),
        };

        let config = Config::try_from(raw).expect("Config parse failed");
        assert_eq!(config.namespace, "default");
        assert_eq!(config.argocds.len(), 6);
        {
            let server_1 = config.mcservers.get("server1").unwrap().try_read().unwrap();
            assert_eq!(server_1.name, "server1_customname");
            assert!(server_1.required_to_start);
        }
        {
            let server_2 = config.mcservers.get("server2").unwrap().try_read().unwrap();
            assert_eq!(server_2.name, "server2");
            assert!(!server_2.required_to_start);
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

    #[test]
    fn test_rawconfig_from_yaml() {
        let raw_yaml = r#"
namespace: "default"
mcproxy:
  name: "mcproxy"
  argocd: "apps/minecraft/mcproxy"
  rcon_container: "mcproxy"
mcservers:
  server1:
    name: "server1_customname"
    argocd: "apps/minecraft/servers/server1"
    rcon_container: "server1"
  server2:
    argocd: "apps/minecraft/servers/server2"
    rcon_container: "server2"
"#;

        let raw: RawConfig = serde_yaml::from_str(raw_yaml).expect("YAML should deserialize");

        let expected = RawConfig {
            namespace: "default".to_string(),
            mcproxy: raw::RawMinecraftChart {
                name: Some("mcproxy".to_string()),
                argocd: "apps/minecraft/mcproxy".to_string(),
                rcon_container: "mcproxy".to_string(),
                jobs_after_snapshot: BTreeMap::new(),
                required_to_start: None,
            },
            mcservers: BTreeMap::from([
                (
                    "server1".to_string(),
                    raw::RawMinecraftChart {
                        name: Some("server1_customname".to_string()),
                        argocd: "apps/minecraft/servers/server1".to_string(),
                        rcon_container: "server1".to_string(),
                        jobs_after_snapshot: BTreeMap::new(),
                        required_to_start: None,
                    },
                ),
                (
                    "server2".to_string(),
                    raw::RawMinecraftChart {
                        name: None,
                        argocd: "apps/minecraft/servers/server2".to_string(),
                        rcon_container: "server2".to_string(),
                        jobs_after_snapshot: BTreeMap::new(),
                        required_to_start: None,
                    },
                ),
            ]),
        };

        assert_eq!(raw, expected);
    }
}
