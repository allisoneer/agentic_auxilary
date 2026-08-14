use crate::ServerConfig;
use crate::TursoAttentionService;
use chrono::DateTime;
use chrono::Timelike;
use chrono::Utc;
use futures_util::future::BoxFuture;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Runtime time source. Tests can supply one clock to the scheduler and worker calls.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Runtime delay source. Scheduler delays are always raced with cancellation.
pub trait Sleeper: Send + Sync {
    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        let now = Utc::now();
        now.with_nanosecond((now.timestamp_subsec_nanos() / 1_000) * 1_000)
            .unwrap_or(now)
    }
}

impl Sleeper for SystemClock {
    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(duration))
    }
}

pub(crate) async fn run(
    service: Arc<TursoAttentionService>,
    config: Arc<ServerConfig>,
    clock: Arc<dyn Clock>,
    sleeper: Arc<dyn Sleeper>,
    shutdown: CancellationToken,
) {
    let mut backoff = config.scheduler_poll_interval;
    loop {
        let result = service
            .fire_due(clock.now(), config.scheduler_batch_size, &shutdown)
            .await;
        if shutdown.is_cancelled() {
            return;
        }
        let delay = match result {
            Ok(()) => {
                backoff = config.scheduler_poll_interval;
                config.scheduler_poll_interval
            }
            Err(error) => {
                tracing::warn!(error = %error, "reminder scheduler pass failed");
                let delay = backoff.min(config.scheduler_error_backoff_max);
                backoff = backoff
                    .checked_mul(2)
                    .unwrap_or(config.scheduler_error_backoff_max)
                    .min(config.scheduler_error_backoff_max);
                delay
            }
        };
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = sleeper.sleep(delay) => {}
        }
    }
}
