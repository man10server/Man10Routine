use std::panic;

use man10_routine::app;
use tracing::error;
use tracing_error::ErrorLayer;
use tracing_error::ExtractSpanTrace;
use tracing_error::SpanTrace;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_thread_ids(true)
        .with_target(false)
        .with_filter(EnvFilter::from_default_env());

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();

    panic::set_hook(Box::new(move |info| {
        error!("{}", info);
        let span_trace = SpanTrace::capture();
        eprintln!("\n{}\n", color_spantrace::colorize(&span_trace));
    }));

    match app().await {
        Ok(_) => {
            std::process::exit(0);
        }
        Err(e) => {
            error!("{e}");
            if let Some(span_trace) = e.span_trace() {
                eprintln!("\n{}\n", color_spantrace::colorize(span_trace));
            } else {
                eprintln!("\nNo span trace available.\n");
            }
            std::process::exit(1);
        }
    }
}
