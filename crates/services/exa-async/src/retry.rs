use backon::ExponentialBuilder;
use std::time::Duration;

/// Creates the default exponential backoff builder for Exa API requests
///
/// Configured with:
/// - Initial interval: 500ms
/// - Max interval: 4s
/// - Max times: 8
/// - Factor: 2.0
/// - Jitter enabled
#[must_use]
pub fn default_backoff_builder() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(500))
        .with_max_delay(Duration::from_secs(4))
        .with_max_times(8)
        .with_factor(2.0)
        .with_jitter()
}

/// Determines if an HTTP status code should trigger a retry
///
/// Retries on: 408, 409, 429, and 5xx
#[must_use]
pub const fn is_retryable_status(code: u16) -> bool {
    matches!(code, 408 | 409 | 429 | 500..=599)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_matrix() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(408));
        assert!(is_retryable_status(409));
        assert!(!is_retryable_status(404));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(403));
        assert!(!is_retryable_status(200));
    }
}
