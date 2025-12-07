use std::sync::{Arc, Weak};
use tokio::sync::RwLock;

pub(crate) type SharedArgoCd = Arc<RwLock<ArgoCd>>;
pub(crate) type WeakArgoCd = Weak<RwLock<ArgoCd>>;

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct ArgoCd {
    pub(crate) name: String,
    pub(crate) path: Vec<String>,
    pub(crate) parent: Option<WeakArgoCd>,
}
impl ArgoCd {
    pub(crate) fn new_app_of_apps(name: String, parent: Option<WeakArgoCd>) -> SharedArgoCd {
        let path = ArgoCd::path(&parent, name.clone());
        Arc::new(RwLock::new(ArgoCd { name, path, parent }))
    }
    pub(crate) fn new_application(name: String, parent: Option<WeakArgoCd>) -> SharedArgoCd {
        let path = ArgoCd::path(&parent, name.clone());
        Arc::new(RwLock::new(ArgoCd { name, path, parent }))
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
