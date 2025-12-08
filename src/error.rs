use std::fmt::Display;

use tracing_error::{ExtractSpanTrace, SpanTrace};

#[derive(Debug)]
pub struct SpannedErr<T> {
    pub err: T,
    pub span_trace: SpanTrace,
}

pub trait SpannedExt<T, E> {
    fn with_span_trace(self) -> Result<T, SpannedErr<E>>;
}

impl<T, E> SpannedExt<T, E> for Result<T, E> {
    fn with_span_trace(self) -> Result<T, SpannedErr<E>> {
        self.map_err(|e| SpannedErr {
            err: e,
            span_trace: SpanTrace::capture(),
        })
    }
}

impl<E> ExtractSpanTrace for SpannedErr<E> {
    fn span_trace(&self) -> Option<&SpanTrace> {
        Some(&self.span_trace)
    }
}

impl<T: Display> Display for SpannedErr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.err, f)
    }
}

impl<U: std::error::Error> std::error::Error for SpannedErr<U> {}
