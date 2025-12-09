use futures::future;
use tokio::select;
use tokio::signal;
use tokio::sync::watch;
use tracing::{info, warn};

pub struct Shutdown {
    rx: watch::Receiver<Option<&'static str>>,
}

impl Shutdown {
    pub fn new() -> Self {
        Self {
            rx: spawn_shutdown_listener(),
        }
    }

    pub fn requested(&self) -> bool {
        self.rx.borrow().is_some()
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_shutdown_listener() -> watch::Receiver<Option<&'static str>> {
    let (shutdown_tx, shutdown_rx) = watch::channel(None);

    tokio::spawn(async move {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate()).ok();

        let term_future = async {
            if let Some(ref mut sigterm) = sigterm {
                sigterm.recv().await;
                Some("SIGTERM")
            } else {
                future::pending::<Option<&'static str>>().await
            }
        };

        select! {
            res = signal::ctrl_c() => {
                if res.is_ok() {
                    info!("Received SIGINT.");
                    let _ = shutdown_tx.send(Some("SIGINT"));
                } else {
                    warn!("Failed to listen for SIGINT: {:?}", res.err());
                }
            }
            _ = term_future => {
                info!("Received SIGTERM.");
                let _ = shutdown_tx.send(Some("SIGTERM"));
            }
        }
    });

    shutdown_rx
}
