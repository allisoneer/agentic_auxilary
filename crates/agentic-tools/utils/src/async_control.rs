//! Async control flow utilities: retry, semaphore, and timeout helpers.

use std::time::Duration;
use thiserror::Error;
use tokio::sync::Semaphore;

/// Error type for async control operations.
#[derive(Debug, Error)]
pub enum AsyncControlError {
    /// The semaphore was closed.
    #[error("Semaphore closed")]
    SemaphoreClosed,

    /// The operation timed out.
    #[error("Timed out after {0}s")]
    Timeout(u64),

    /// An operation-specific error occurred.
    #[error("{0}")]
    Operation(String),
}

/// Generic helper that acquires a semaphore permit and wraps an operation in a timeout.
///
/// This is a testable building block: tests can inject a local semaphore and short timeout
/// to verify behavior without real Claude sessions.
///
/// # Errors
///
/// Returns an error if:
/// - The semaphore is closed
/// - The operation times out
/// - The operation itself returns an error
pub async fn with_permit_and_timeout<F, Fut, T, E>(
    semaphore: &Semaphore,
    timeout_dur: Duration,
    op: F,
) -> Result<T, AsyncControlError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let _permit = semaphore
        .acquire()
        .await
        .map_err(|_| AsyncControlError::SemaphoreClosed)?;

    match tokio::time::timeout(timeout_dur, op()).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(AsyncControlError::Operation(e.to_string())),
        Err(_) => Err(AsyncControlError::Timeout(timeout_dur.as_secs())),
    }
}

/// Generic retry helper with fixed delays.
///
/// This is a testable building block: tests can inject a custom sleep function
/// to verify retry behavior without real waits.
///
/// # Arguments
///
/// * `delays` - Slice of durations to wait before each attempt (first attempt uses delays[0])
/// * `sleep_fn` - Async function to call for sleeping
/// * `op` - The operation to retry
///
/// # Errors
///
/// Returns the last error from the operation if all retries fail.
pub async fn retry_fixed_delays<F, Fut, SleepFn, SleepFut, T, E>(
    delays: &[Duration],
    mut sleep_fn: SleepFn,
    mut op: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    SleepFn: FnMut(Duration) -> SleepFut,
    SleepFut: std::future::Future<Output = ()>,
    E: std::fmt::Debug,
{
    let mut last_err = None;

    for d in delays {
        sleep_fn(*d).await;

        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    // Preserve the last underlying error
    Err(last_err.expect("retry_fixed_delays called with empty delays"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct TestError(String);

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    #[tokio::test]
    async fn semaphore_limits_concurrency() {
        let semaphore = Semaphore::new(2);
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_observed = Arc::new(AtomicUsize::new(0));

        let mut handles = vec![];
        for _ in 0..4 {
            let sem = &semaphore;
            let in_flight = Arc::clone(&in_flight);
            let max_observed = Arc::clone(&max_observed);

            handles.push(async move {
                let result: Result<(), AsyncControlError> =
                    with_permit_and_timeout(sem, Duration::from_secs(10), || async {
                        let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_observed.fetch_max(current, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        in_flight.fetch_sub(1, Ordering::SeqCst);
                        Ok::<_, TestError>(())
                    })
                    .await;
                result
            });
        }

        futures::future::join_all(handles).await;

        // Max in-flight should be exactly 2 (the semaphore limit)
        assert_eq!(max_observed.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn timeout_returns_error_when_exceeded() {
        let semaphore = Semaphore::new(1);

        let result: Result<(), AsyncControlError> =
            with_permit_and_timeout(&semaphore, Duration::from_millis(10), || async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, TestError>(())
            })
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AsyncControlError::Timeout(_) => {}
            other => panic!("Expected Timeout error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn timeout_returns_success_when_op_completes_in_time() {
        let semaphore = Semaphore::new(1);

        let result: Result<i32, AsyncControlError> =
            with_permit_and_timeout(&semaphore, Duration::from_secs(10), || async {
                Ok::<_, TestError>(42)
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn retry_succeeds_on_third_attempt() {
        let attempt_count = Arc::new(AtomicUsize::new(0));
        let delays_observed = Arc::new(std::sync::Mutex::new(Vec::new()));

        let delays = [
            Duration::from_millis(0),
            Duration::from_millis(10),
            Duration::from_millis(20),
        ];

        let result: Result<&str, TestError> = retry_fixed_delays(
            &delays,
            |d| {
                let delays_observed = Arc::clone(&delays_observed);
                async move {
                    delays_observed.lock().unwrap().push(d);
                }
            },
            || {
                let attempt_count = Arc::clone(&attempt_count);
                async move {
                    let attempt = attempt_count.fetch_add(1, Ordering::SeqCst) + 1;
                    if attempt < 3 {
                        Err(TestError(format!("attempt {attempt} failed")))
                    } else {
                        Ok("success")
                    }
                }
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_returns_last_error_when_all_fail() {
        let delays = [Duration::from_millis(0), Duration::from_millis(0)];

        let result: Result<(), TestError> = retry_fixed_delays(
            &delays,
            |_| async {},
            || async { Err(TestError("always fails".into())) },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, "always fails");
    }

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        let delays = [Duration::from_millis(0)];

        let result: Result<i32, TestError> =
            retry_fixed_delays(&delays, |_| async {}, || async { Ok(42) }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }
}
