use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::error::{AppError, AppResult};

/// Initializes global tracing: pretty logs to stdout, JSON logs (daily
/// rotated) to `log_dir`. The returned [`WorkerGuard`] must be held for the
/// lifetime of the process — dropping it stops the file writer from flushing.
///
/// Sensitive fields (API keys, tokens) must never be passed to `tracing`
/// macros directly; redact them at the call site before logging.
pub fn init(log_dir: &Path, log_level: &str) -> AppResult<WorkerGuard> {
    std::fs::create_dir_all(log_dir)?;

    let file_appender = tracing_appender::rolling::daily(log_dir, "tf2-terminal.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_new(log_level)
        .map_err(|e| AppError::Config(format!("invalid log level '{log_level}': {e}")))?;

    let stdout_layer = fmt::layer().with_target(true).with_ansi(true);
    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .map_err(|e| AppError::Internal(format!("failed to init tracing: {e}")))?;

    Ok(guard)
}
