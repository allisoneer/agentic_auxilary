use futures::future::BoxFuture;
use std::sync::Arc;
use tokio::sync::OnceCell;

type ReadinessCheck = Arc<dyn Fn() -> BoxFuture<'static, anyhow::Result<()>> + Send + Sync>;

#[derive(Clone)]
pub struct ThoughtsMcpReadinessGate {
    inner: Arc<Inner>,
}

struct Inner {
    ready: OnceCell<()>,
    check: ReadinessCheck,
}

impl ThoughtsMcpReadinessGate {
    pub fn new() -> Self {
        Self::new_with_check(|| {
            Box::pin(async { thoughts_tool::workspace::ensure_thoughts_environment_ready().await })
        })
    }

    pub fn new_with_check<F>(check: F) -> Self
    where
        F: Fn() -> BoxFuture<'static, anyhow::Result<()>> + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(Inner {
                ready: OnceCell::new(),
                check: Arc::new(check),
            }),
        }
    }

    pub async fn ensure_ready(&self) -> anyhow::Result<()> {
        let check = Arc::clone(&self.inner.check);
        self.inner
            .ready
            .get_or_try_init(|| async move {
                (check)().await?;
                Ok::<(), anyhow::Error>(())
            })
            .await?;
        Ok(())
    }
}

impl Default for ThoughtsMcpReadinessGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use tokio::sync::Barrier;

    #[tokio::test]
    async fn caches_success_for_process_lifetime() {
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = ThoughtsMcpReadinessGate::new_with_check({
            let calls = Arc::clone(&calls);
            move || {
                let calls = Arc::clone(&calls);
                Box::pin(async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }
        });

        gate.ensure_ready().await.unwrap();
        gate.ensure_ready().await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_first_calls_use_single_flight() {
        let calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let gate = ThoughtsMcpReadinessGate::new_with_check({
            let calls = Arc::clone(&calls);
            let barrier = Arc::clone(&barrier);
            move || {
                let calls = Arc::clone(&calls);
                let barrier = Arc::clone(&barrier);
                Box::pin(async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    barrier.wait().await;
                    Ok(())
                })
            }
        });

        let first = {
            let gate = gate.clone();
            tokio::spawn(async move { gate.ensure_ready().await.unwrap() })
        };
        let second = {
            let gate = gate.clone();
            tokio::spawn(async move { gate.ensure_ready().await.unwrap() })
        };

        barrier.wait().await;
        first.await.unwrap();
        second.await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failures_remain_retryable() {
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = ThoughtsMcpReadinessGate::new_with_check({
            let calls = Arc::clone(&calls);
            move || {
                let calls = Arc::clone(&calls);
                Box::pin(async move {
                    let current = calls.fetch_add(1, Ordering::SeqCst);
                    if current == 0 {
                        anyhow::bail!("not ready yet");
                    }
                    Ok(())
                })
            }
        });

        assert!(gate.ensure_ready().await.is_err());
        gate.ensure_ready().await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
