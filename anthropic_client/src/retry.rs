use reqwest::header::HeaderMap;
use std::time::Duration;

/// Creates the default exponential backoff configuration
///
/// Configured with:
/// - Initial interval: 500ms
/// - Max interval: 8s
/// - Max elapsed time: 60s
/// - Randomization factor: 0.25
/// - Multiplier: 2.0
#[must_use]
pub fn default_backoff() -> backoff::ExponentialBackoff {
    backoff::ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(60)),
        initial_interval: Duration::from_millis(500),
        max_interval: Duration::from_secs(8),
        randomization_factor: 0.25,
        multiplier: 2.0,
        ..Default::default()
    }
}

/// Determines if an HTTP status code should trigger a retry
///
/// Retries on: 408, 409, 429, 5xx, and 529 (overloaded)
#[must_use]
pub const fn is_retryable_status(code: u16) -> bool {
    matches!(code, 408 | 409 | 429 | 500..=599) || code == 529
}

/// Parses the `Retry-After` or `retry-after-ms` header from the response
///
/// Returns the duration to wait before retrying, capped at 60 seconds.
/// Returns `None` if the header is missing or malformed.
#[must_use]
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    // Try retry-after-ms first (Anthropic specific)
    if let Some(v) = headers.get("retry-after-ms")
        && let Ok(s) = v.to_str()
        && let Ok(ms) = s.parse::<u64>()
    {
        return Some(Duration::from_millis(ms.min(60_000)));
    }

    // Standard retry-after in seconds
    if let Some(v) = headers.get("retry-after")
        && let Ok(s) = v.to_str()
        && let Ok(secs) = s.parse::<u64>()
    {
        return Some(Duration::from_secs(secs.min(60)));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_matrix() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(529));
        assert!(is_retryable_status(408));
        assert!(is_retryable_status(409));
        assert!(!is_retryable_status(404));
        assert!(!is_retryable_status(400));
    }

    #[test]
    fn retry_after_parse() {
        let mut h = HeaderMap::new();
        h.insert("retry-after", "120".parse().unwrap());
        let d = parse_retry_after(&h).expect("Should parse retry-after");
        assert_eq!(d.as_secs(), 60); // capped at 60
    }

    #[test]
    fn retry_after_ms() {
        let mut h = HeaderMap::new();
        h.insert("retry-after-ms", "5000".parse().unwrap());
        let d = parse_retry_after(&h).expect("Should parse retry-after-ms");
        assert_eq!(d.as_millis(), 5000);
    }
}
