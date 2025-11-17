use std::time::Duration;

#[must_use]
pub fn default_backoff() -> backoff::ExponentialBackoff {
    backoff::ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(60)),
        initial_interval: Duration::from_millis(200),
        max_interval: Duration::from_secs(10),
        randomization_factor: 0.5,
        multiplier: 2.0,
        ..Default::default()
    }
}
