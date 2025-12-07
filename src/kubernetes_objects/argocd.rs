use std::sync::{Arc, Weak};
use tokio::sync::RwLock;

use super::minecraft_chart::MinecraftChart;

#[derive(Debug)]
pub(crate) enum ArgoCD {
    AppOfApps {
        parent: RwLock<Weak<Option<ArgoCD>>>,
        children: RwLock<Vec<Arc<ArgoCD>>>,
    },
    Application {
        parent: RwLock<Weak<Option<ArgoCD>>>,
        chart: RwLock<Weak<MinecraftChart>>,
    },
}
