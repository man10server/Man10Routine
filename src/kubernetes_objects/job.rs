use k8s_openapi::api::batch::v1::Job;

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct CustomJob {
    /// Internal job name
    pub(crate) name: String,

    /// Names of jobs that must complete before this job starts
    pub(crate) dependencies: Vec<String>,

    /// Kubernetes Job YAML
    pub(crate) manifest: Job,
}
