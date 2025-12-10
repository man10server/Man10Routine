use k8s_openapi::api::batch::v1::Job;

use crate::config::polling::PollingConfig;

#[derive(Debug, Clone)]
pub(crate) struct CustomJob {
    /// Names of jobs that must complete before this job starts
    pub(crate) dependencies: Vec<String>,

    /// Kubernetes Job YAML
    pub(crate) manifest: Job,

    /// Whether the job's successful completion is required to continue routine or not
    pub(crate) required: bool,

    /// Polling configuration for waiting for job completion
    pub(crate) completion_polling: PollingConfig,
}
