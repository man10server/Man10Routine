use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{ArgoCdError, SharedArgoCd, WeakArgoCd};
use derive_debug::Dbg;
use json_patch::jsonptr::PointerBuf;
use kcr_argoproj_io::v1alpha1::applications::{Application, ApplicationSyncPolicy};
use kube::api::{Patch, PatchParams};
use kube::{Api, Client};
use serde_json::json;
use tokio::sync::RwLock;
use tracing::field::Empty;
use tracing::{Instrument, Level, Span, error, info, trace_span};
use tracing_error::ExtractSpanTrace;

use crate::kubernetes_objects::{ARGOCD_NAMESPACE, MANAGEER_ROLE_NAME};

#[derive(Dbg)]
pub(super) struct Teardown {
    argocd: WeakArgoCd,
    upstream: Option<TearingArgoCdGuard>,
    original_sync_policy: Option<ApplicationSyncPolicy>,
    #[dbg(skip)]
    client: Client,
    counter: AtomicUsize,
}

#[derive(Debug)]
pub(crate) struct TearingArgoCdGuard {
    tear: Arc<RwLock<Teardown>>,
    dropped: bool,
}

pub(crate) trait TearingArgoCd {
    async fn tear(&self, client: Client) -> Result<TearingArgoCdGuard, ArgoCdError>;
}

impl TearingArgoCd for SharedArgoCd {
    #[tracing::instrument(
        "argocd/tear",
        level = Level::TRACE,
        skip(self, client),
        fields(argocd_name = %self.read().await.name)
    )]
    async fn tear(&self, client: Client) -> Result<TearingArgoCdGuard, ArgoCdError> {
        if let Some(tear) = self.read().await.tear.as_ref() {
            return match tear {
                Ok(tear) => Ok(tear.generate_guard().await),
                Err(e) => Err(e.clone()),
            };
        }

        let write_guard = &mut self.write().await;
        let name = &write_guard.name;

        let upstream = match &write_guard.parent_upgrade() {
            Some(parent) => Some(Box::pin(parent.tear(client.clone())).await?),
            None => None,
        };

        let original_sync_policy = match sync_teardown(name, client.clone()).await {
            Ok(policy) => policy,
            Err(e) => {
                if let Some(upstream) = upstream
                    && let Err(e) = upstream.close().await
                {
                    error!(
                        "Failed to rollback upstream ArgoCd teardown after failing to sync_teardown: {}",
                        e
                    );
                    if let Some(span_trace) = e.span_trace() {
                        eprintln!("\n{}", color_spantrace::colorize(span_trace));
                    }
                }
                return Err(e);
            }
        };

        info!("ArgoCD application '{}' was successfully torn down.", name);

        let tear = Arc::new(RwLock::new(Teardown {
            argocd: Arc::downgrade(self),
            upstream,
            original_sync_policy,
            client: client.clone(),
            counter: AtomicUsize::new(0),
        }));

        write_guard.tear = Some(Ok(tear.clone()));

        Ok(tear.generate_guard().await)
    }
}

impl TearingArgoCdGuard {
    #[tracing::instrument(
        "tearing_argocd_guard/close",
        level = Level::TRACE,
        skip(self),
        fields(counter = Empty)
    )]
    pub(crate) async fn close(mut self) -> Result<(), ArgoCdError> {
        self.dropped = true;
        let c = self
            .tear
            .read()
            .await
            .counter
            .fetch_sub(1, Ordering::SeqCst);
        Span::current().record("counter", c);
        if c == 1 {
            let mut teardown = self.tear.write().await;
            let argocd = &teardown.argocd.upgrade().ok_or_else(ArgoCdError::dropped)?;
            let write_guard = &mut argocd.write().await;

            sync_tearup(
                &write_guard.name,
                teardown.client.clone(),
                teardown.original_sync_policy.clone(),
            )
            .await?;

            info!(
                "ArgoCD application '{}' was successfully restored.",
                write_guard.name
            );

            if let Some(upstream) = teardown.upstream.take() {
                Box::pin(upstream.close()).await?;
            }
            write_guard.tear = None;
        }
        Ok(())
    }
}

impl Drop for TearingArgoCdGuard {
    fn drop(&mut self) {
        if !self.dropped {
            panic!(
                "TearingArgoCdGuard for ArgoCd '{}' was dropped without calling close()",
                self.tear
                    .try_read()
                    .ok()
                    .and_then(|t| t.argocd.upgrade())
                    .and_then(|a| a.try_read().ok().map(|ag| ag.name.clone()))
                    .unwrap_or_else(|| "<dropped>".to_string())
            );
        }
    }
}

trait TeardownExt {
    async fn generate_guard(&self) -> TearingArgoCdGuard;
}
impl TeardownExt for Arc<RwLock<Teardown>> {
    async fn generate_guard(&self) -> TearingArgoCdGuard {
        self.read().await.counter.fetch_add(1, Ordering::SeqCst);
        TearingArgoCdGuard {
            tear: self.clone(),
            dropped: false,
        }
    }
}

#[tracing::instrument(
    "argocd/sync_teardown",
    skip(name, client),
    fields(argocd_name = %name)
)]
async fn sync_teardown(
    name: &str,
    client: Client,
) -> Result<Option<ApplicationSyncPolicy>, ArgoCdError> {
    let api: Api<Application> = Api::namespaced(client, ARGOCD_NAMESPACE);

    let app = async { api.get(name).await.map_err(ArgoCdError::from) }
        .instrument(trace_span!("get_argocd_application"))
        .await?;

    let original = app.spec.sync_policy;

    async {
        let params = PatchParams::apply(MANAGEER_ROLE_NAME);
        let patch = Patch::Json::<()>(json_patch::Patch(vec![json_patch::PatchOperation::Remove(
            json_patch::RemoveOperation {
                path: PointerBuf::parse("/spec/syncPolicy/automated").unwrap(),
            },
        )]));
        api.patch(name, &params, &patch)
            .await
            .map_err(ArgoCdError::from)
    }
    .instrument(trace_span!("patch_argocd_application"))
    .await?;

    Ok(original)
}

#[tracing::instrument(
    "argocd/sync_tearup",
    skip_all,
    fields(argocd_name = %name, original_sync_policy = ?original_sync_policy)
)]
async fn sync_tearup(
    name: &str,
    client: Client,
    original_sync_policy: Option<ApplicationSyncPolicy>,
) -> Result<(), ArgoCdError> {
    let api: Api<Application> = Api::namespaced(client, ARGOCD_NAMESPACE);
    let patch = json!({
        "apiVersion": "argoproj.io/v1alpha1",
        "kind": "Application",
        "metadata": {
            "name": name,
            "namespace": ARGOCD_NAMESPACE,
        },
        "spec": {
            "syncPolicy": original_sync_policy,
        }
    });
    async {
        let params = PatchParams::apply(MANAGEER_ROLE_NAME);
        let patch = Patch::Apply(&patch);
        api.patch(name, &params, &patch)
            .await
            .map_err(ArgoCdError::from)
    }
    .instrument(trace_span!("patch_argocd_application"))
    .await?;
    Ok(())
}
