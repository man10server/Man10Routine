use super::{ArgoCd, SharedArgoCd, WeakArgoCd};
use std::sync::Arc;
use tokio::sync::RwLock;

impl ArgoCd {
    pub(crate) fn new_app_of_apps(name: String, parent: Option<WeakArgoCd>) -> SharedArgoCd {
        let path = ArgoCd::path(&parent, name.clone());
        Arc::new(RwLock::new(ArgoCd {
            name,
            path,
            parent,
            tear: None,
        }))
    }
    pub(crate) fn new_application(name: String, parent: Option<WeakArgoCd>) -> SharedArgoCd {
        let path = ArgoCd::path(&parent, name.clone());
        Arc::new(RwLock::new(ArgoCd {
            name,
            path,
            parent,
            tear: None,
        }))
    }
    fn path(parent: &Option<WeakArgoCd>, name: String) -> Vec<String> {
        let mut parent_path = match parent {
            Some(weak_parent) => weak_parent
                .upgrade()
                .expect("initializing ArgoCd failed: Parent ArgoCD object has been dropped")
                .try_read()
                .expect("initializing ArgoCd failed: ArgoCD RwLock poisoned")
                .path
                .clone(),
            None => vec![],
        };
        parent_path.push(name);
        parent_path
    }
}

#[cfg(test)]
mod tests {

    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_argocd_path() {
        let app_of_apps = ArgoCd::new_app_of_apps("root".to_string(), None);
        let application =
            ArgoCd::new_application("app1".to_string(), Some(Arc::downgrade(&app_of_apps)));
        assert_eq!(app_of_apps.try_read().unwrap().path, vec!["root"]);
        assert_eq!(application.try_read().unwrap().path, vec!["root", "app1"]);
    }
}
