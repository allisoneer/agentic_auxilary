use std::io;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;

/// A guard that temporarily disables logging output.
/// When dropped, the original logging configuration is restored.
pub struct TuiLogGuard {
    _guard: tracing::subscriber::DefaultGuard,
}

impl TuiLogGuard {
    /// Creates a new TUI log guard that disables all logging output.
    /// The original logging configuration is restored when this guard is dropped.
    pub fn new() -> Self {
        // Create a no-op subscriber that discards all logs
        let noop_subscriber = tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(io::sink) // Write to /dev/null equivalent
                    .with_target(false)
                    .with_thread_ids(false)
                    .with_thread_names(false),
            );

        // Set the no-op subscriber as the default for this scope
        let guard = tracing::subscriber::set_default(noop_subscriber);

        Self { _guard: guard }
    }
}

/// Temporarily disables logging for TUI operations.
/// Returns a guard that restores logging when dropped.
pub fn disable_logging_for_tui() -> TuiLogGuard {
    TuiLogGuard::new()
}