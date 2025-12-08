pub(crate) mod initialize;
pub(crate) mod tearing;

use std::sync::{Arc, Weak};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing_error::{ExtractSpanTrace, SpanTrace};

use self::tearing::Teardown;

pub(crate) type SharedArgoCd = Arc<RwLock<ArgoCd>>;
pub(crate) type WeakArgoCd = Weak<RwLock<ArgoCd>>;

#[derive(Debug)]
pub(crate) struct ArgoCd {
    pub(crate) name: String,
    pub(crate) path: Vec<String>,
    pub(crate) parent: Option<WeakArgoCd>,
    tear: Option<Result<Arc<RwLock<Teardown>>, ArgoCdError>>,
}

impl ArgoCd {
    fn parent_upgrade(&self) -> Option<SharedArgoCd> {
        let Some(shared) = self.parent.as_ref()?.upgrade() else {
            panic!(
                "ArgoCd object '{}' has a parent, but the parent has been dropped",
                self.name
            );
        };
        Some(shared)
    }
}

#[derive(Error, Debug, Clone)]
pub enum ArgoCdError {
    #[error("Kubernetes API error: {0}")]
    KubeError(String, SpanTrace),

    #[error("Argocd application was already dropped")]
    Dropped(SpanTrace),
}

impl ExtractSpanTrace for ArgoCdError {
    fn span_trace(&self) -> Option<&SpanTrace> {
        match self {
            ArgoCdError::KubeError(_, s) => Some(s),
            ArgoCdError::Dropped(s) => Some(s),
        }
    }
}

impl From<kube::Error> for ArgoCdError {
    fn from(err: kube::Error) -> Self {
        ArgoCdError::KubeError(err.to_string(), SpanTrace::capture())
    }
}

impl ArgoCdError {
    pub(crate) fn dropped() -> Self {
        ArgoCdError::Dropped(SpanTrace::capture())
    }
}
