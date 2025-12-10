use std::collections::BTreeMap;

use super::Config;
use super::polling::PollingConfig;
use crate::kubernetes_objects::argocd::SharedArgoCd;
use crate::kubernetes_objects::custom_job::CustomJob;
use crate::kubernetes_objects::minecraft_chart::MinecraftChart;
use k8s_openapi::api::batch::v1::Job;
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

    /// Custom jobs that have been created after snapshot of the volumes were taken
    #[serde(default)]
    pub(super) jobs_after_snapshot: BTreeMap<String, RawCustomJob>,

    /// Whether this chart is required to restart the mcproxy
    pub(super) required_to_start: Option<bool>,
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Deserialize, Debug, Clone)]
pub(super) struct RawCustomJob {
    /// Names of jobs that must complete before this job starts
    #[serde(default)]
    pub(super) dependencies: Vec<String>,

    /// Kubernetes Job YAML
    pub(super) manifest: Job,

    /// Whether the job's successful completion is required to continue routine or not
    #[serde(default = "default_required")]
    pub(super) required: bool,

    /// Polling configuration for waiting for job completion
    #[serde(default)]
    pub(super) completion_polling: PollingConfig,
}

const fn default_required() -> bool {
    true
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

    #[error("mcserver key '{name}' must not contain '/' characters")]
    McserverNameIncludesSlash { name: String },

    #[error("The 'required_to_start' field cannot be set for mcproxy")]
    RequiredToStartCannotBeSetForMcproxy,

    #[error("Not all 'required_to_start' values for all mcservers should be false.")]
    McproxyRequiresNoServerToStart,

    #[error("Job name '{job_name}' in chart '{chart_name}' must not contain '/' characters")]
    JobNameIncludesSlash {
        chart_name: String,
        job_name: String,
    },
}

impl TryFrom<RawConfig> for Config {
    type Error = ConfigParseError;
    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        let mut argocds: BTreeMap<String, SharedArgoCd> = BTreeMap::new();

        for name in raw.mcservers.keys() {
            if name.contains('/') {
                return Err(ConfigParseError::McserverNameIncludesSlash { name: name.clone() });
            }
        }

        if raw.mcproxy.required_to_start.is_some() {
            return Err(ConfigParseError::RequiredToStartCannotBeSetForMcproxy);
        }

        if !raw
            .mcservers
            .values()
            .any(|s| s.required_to_start.unwrap_or(true))
        {
            return Err(ConfigParseError::McproxyRequiresNoServerToStart);
        }

        let namespace = raw.namespace;
        let mcproxy_argocd = Self::build_argocd_hierarchy(&mut argocds, &raw.mcproxy.argocd)?;
        let mcproxy_name = raw
            .mcproxy
            .name
            .ok_or(ConfigParseError::McproxyNameMissing)?;
        let mcproxy_jobs =
            Self::build_jobs_after_snapshot(raw.mcproxy.jobs_after_snapshot, &mcproxy_name)?;
        let mcproxy = MinecraftChart::new(
            mcproxy_name,
            mcproxy_argocd,
            raw.mcproxy.rcon_container,
            mcproxy_jobs,
            false,
        );
        let mcservers = raw
            .mcservers
            .into_iter()
            .map(|(name, server)| {
                let server_argocd = Self::build_argocd_hierarchy(&mut argocds, &server.argocd)?;
                let server_name = server.name.unwrap_or_else(|| name.clone());
                let jobs_after_snapshot =
                    Self::build_jobs_after_snapshot(server.jobs_after_snapshot, &server_name)?;
                let mc_chart = MinecraftChart::new(
                    server_name,
                    server_argocd,
                    server.rcon_container,
                    jobs_after_snapshot,
                    server.required_to_start.unwrap_or(true),
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

impl Config {
    fn build_jobs_after_snapshot(
        raw_jobs: BTreeMap<String, RawCustomJob>,
        chart_name: &str,
    ) -> Result<BTreeMap<String, CustomJob>, ConfigParseError> {
        raw_jobs
            .into_iter()
            .map(|(name, job)| {
                if name.contains('/') {
                    return Err(ConfigParseError::JobNameIncludesSlash {
                        chart_name: chart_name.to_string(),
                        job_name: name,
                    });
                }
                Ok((
                    name.clone(),
                    CustomJob {
                        dependencies: job.dependencies,
                        manifest: job.manifest,
                        required: job.required,
                        completion_polling: job.completion_polling,
                    },
                ))
            })
            .collect()
    }
}
