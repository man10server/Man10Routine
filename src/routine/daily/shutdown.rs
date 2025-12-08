use futures::future;
use tokio::select;
use tokio::sync::watch;
use tokio::time::Duration;
use tokio::{signal, time};
use tracing::{info, warn};

use super::error::DailyRoutineError;

pub(super) struct Shutdown {
    rx: watch::Receiver<Option<&'static str>>,
}

impl Shutdown {
    pub(super) fn new() -> Self {
        Self {
            rx: spawn_shutdown_listener(),
        }
    }

    pub(super) fn requested(&self) -> bool {
        self.rx.borrow().is_some()
    }

    pub(super) async fn sleep_or_shutdown(&mut self, duration: Duration) -> bool {
        if self.requested() {
            return true;
        }

        select! {
            _ = time::sleep(duration) => false,
            changed = self.rx.changed() => {
                changed.is_ok()
            }
        }
    }

    pub(super) async fn run_phase_with_shutdown<F>(
        &mut self,
        phase_name: &str,
        phase_future: F,
    ) -> Result<(), DailyRoutineError>
    where
        F: std::future::Future<Output = Result<(), DailyRoutineError>>,
    {
        if self.requested() {
            info!("Shutdown already requested before starting {phase_name}. Skipping the phase.");
            return Ok(());
        }

        tokio::pin!(phase_future);

        select! {
            res = &mut phase_future => res,
            changed = self.rx.changed() => {
                if changed.is_ok() {
                    if let Some(signal) = *self.rx.borrow() {
                        info!("Received {signal}. Finishing {phase_name} before shutting down...");
                    } else {
                        info!("Shutdown requested. Finishing {phase_name} before shutting down...");
                    }
                }
                phase_future.await
            }
        }
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
