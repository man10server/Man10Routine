use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{ArgoCdError, SharedArgoCd, WeakArgoCd};
use derive_debug::Dbg;
use kcr_argoproj_io::v1alpha1::applications::{Application, ApplicationSyncPolicy};
use kube::api::{Patch, PatchParams};
use kube::{Api, Client};
use serde_json::json;
use tokio::sync::RwLock;

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
    async fn tear(&self, client: Client) -> Result<TearingArgoCdGuard, ArgoCdError> {
        if let Some(tear) = self.read().await.tear.as_ref() {
            return match tear {
                Ok(tear) => Ok(tear.generate_guard().await),
                Err(e) => Err(e.clone()),
            };
        }

        let write_guard = &mut self.write().await;

        let upstream = match &write_guard.parent_upgrade() {
            Some(parent) => Some(Box::pin(parent.tear(client.clone())).await?),
            None => None,
        };

        let name = &write_guard.name;
        let original_sync_policy = match sync_teardown(name, client.clone()).await {
            Ok(policy) => policy,
            Err(e) => {
                if let Some(upstream) = upstream
                    && let Err(e) = upstream.close().await
                {
                    eprintln!(
                        "Failed to rollback upstream ArgoCd teardown after failing to \
                             sync_teardown for '{}': {}",
                        name, e
                    );
                }
                return Err(e);
            }
        };

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
    pub(crate) async fn close(mut self) -> Result<(), ArgoCdError> {
        let c = self
            .tear
            .read()
            .await
            .counter
            .fetch_sub(1, Ordering::SeqCst);
        if c == 1 {
            let mut teardown = self.tear.write().await;
            let argocd = &teardown.argocd.upgrade().ok_or(ArgoCdError::Dropped)?;
            let write_guard = &mut argocd.write().await;

            sync_tearup(
                &write_guard.name,
                teardown.client.clone(),
                teardown.original_sync_policy.clone(),
            )
            .await?;
            if let Some(upstream) = teardown.upstream.take() {
                Box::pin(upstream.close()).await?;
            }
            write_guard.tear = None;
        }
        self.dropped = true;
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

async fn sync_teardown(
    name: &str,
    client: Client,
) -> Result<Option<ApplicationSyncPolicy>, ArgoCdError> {
    let api: Api<Application> = Api::namespaced(client, ARGOCD_NAMESPACE);

    let app = api.get(name).await?;

    let original = app.spec.sync_policy;

    let patch = json!({
        "spec": {
            "syncPolicy": null,
        }
    });

    let params = PatchParams::apply(MANAGEER_ROLE_NAME);
    let patch = Patch::Apply(&patch);
    api.patch(name, &params, &patch).await?;

    Ok(original)
}

async fn sync_tearup(
    name: &str,
    client: Client,
    original_sync_policy: Option<ApplicationSyncPolicy>,
) -> Result<(), ArgoCdError> {
    let api: Api<Application> = Api::namespaced(client, ARGOCD_NAMESPACE);
    let patch = json!({
        "spec": {
            "syncPolicy": original_sync_policy,
        }
    });
    let params = PatchParams::apply(MANAGEER_ROLE_NAME);
    let patch = Patch::Apply(&patch);
    api.patch(name, &params, &patch).await?;
    Ok(())
}
