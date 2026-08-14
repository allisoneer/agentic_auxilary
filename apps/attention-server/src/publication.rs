use attention_kernel as kernel;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

#[derive(Clone)]
pub struct PublicationHub {
    inner: Arc<Mutex<Inner>>,
    capacity: usize,
}
struct Subscriber {
    events: mpsc::Sender<kernel::ChangeEvent>,
    overflow: Option<oneshot::Sender<()>>,
}
struct Inner {
    next: u64,
    subscribers: HashMap<u64, Subscriber>,
}
pub struct PublicationSubscription {
    id: u64,
    hub: PublicationHub,
    pub receiver: mpsc::Receiver<kernel::ChangeEvent>,
    pub overflow: oneshot::Receiver<()>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublishOutcome {
    pub delivered: usize,
    pub overflowed: usize,
    pub closed: usize,
}

impl PublicationHub {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                next: 1,
                subscribers: HashMap::new(),
            })),
            capacity,
        }
    }
    pub fn subscribe(&self) -> PublicationSubscription {
        let (tx, receiver) = mpsc::channel(self.capacity);
        let (overflow_tx, overflow) = oneshot::channel();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = inner.next;
        inner.next = inner.next.saturating_add(1);
        inner.subscribers.insert(
            id,
            Subscriber {
                events: tx,
                overflow: Some(overflow_tx),
            },
        );
        PublicationSubscription {
            id,
            hub: self.clone(),
            receiver,
            overflow,
        }
    }

    /// Publishes one committed event and explicitly reports slow and closed subscribers.
    pub fn publish(&self, event: &kernel::ChangeEvent) -> PublishOutcome {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut outcome = PublishOutcome::default();
        inner.subscribers.retain(
            |_, subscriber| match subscriber.events.try_send(event.clone()) {
                Ok(()) => {
                    outcome.delivered += 1;
                    true
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    outcome.overflowed += 1;
                    if let Some(signal) = subscriber.overflow.take() {
                        let _ = signal.send(());
                    }
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    outcome.closed += 1;
                    false
                }
            },
        );
        outcome
    }
}
impl Drop for PublicationSubscription {
    fn drop(&mut self) {
        self.hub
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .subscribers
            .remove(&self.id);
    }
}
