use std::path::Path;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize logging with console output only.
/// Uses RUST_LOG env var for filtering (defaults to INFO).
pub fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .init();
}

/// Initialize logging with both console and file output.
/// Returns a guard that must be kept alive for the duration of the application
/// to ensure all log messages are flushed to the file.
pub fn init_logging_with_file(log_dir: &Path) -> WorkerGuard {
    let file_appender = tracing_appender::rolling::daily(log_dir, "bitosdt.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_ansi(true),
        )
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_ansi(false)
                .with_writer(file_writer),
        )
        .init();

    guard
}

/// Initialize logging at a specific level (useful for CLI --verbose flags).
pub fn init_logging_with_level(level: Level) {
    let filter = match level {
        Level::TRACE => "trace",
        Level::DEBUG => "debug",
        Level::INFO => "info",
        Level::WARN => "warn",
        Level::ERROR => "error",
    };

    let env_filter = EnvFilter::new(filter);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .init();
}
