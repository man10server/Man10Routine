use self::cli::{Cli, Routine};
use self::routine::daily::DailyRoutineContext;
use clap::Parser;
use thiserror::Error;
use tracing::info;
use tracing_error::ExtractSpanTrace;
use tracing_error::SpanTrace;

pub mod cli;
pub mod config;
pub mod error;
pub mod kubernetes_objects;
pub(crate) mod routine;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Failed to load config.\n{0}")]
    ConfigError(#[from] config::ConfigLoadError),

    #[error("Failed to initialize kubernetes client.\n{0}")]
    KubeClientError(#[from] kube::Error),

    #[error("Daily routine stopped due to following error:\n{0}")]
    DailyRoutineError(#[from] crate::routine::daily::error::DailyRoutineError),
}

impl ExtractSpanTrace for AppError {
    fn span_trace(&self) -> Option<&SpanTrace> {
        match self {
            AppError::DailyRoutineError(e) => e.span_trace(),
            _ => None,
        }
    }
}

pub async fn app() -> Result<(), AppError> {
    let cli = Cli::parse();
    let config = config::Config::new_from_file(&cli.config).await?;

    info!("Config Loaded.");

    let client = kube::Client::try_default().await?;

    info!("Kubernetes Client Initialized.");

    match cli.routine {
        Routine::Daily {} => {
            let mut context = DailyRoutineContext::new(config, client);
            context.run().await?;
        }
    }

    Ok(())
}
