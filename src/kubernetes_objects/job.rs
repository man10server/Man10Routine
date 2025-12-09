use k8s_openapi::api::batch::v1::Job;

#[derive(Debug, Clone)]
pub(crate) struct CustomJob {
    /// Names of jobs that must complete before this job starts
    pub(crate) dependencies: Vec<String>,

    /// Kubernetes Job YAML
    pub(crate) manifest: Job,
}
