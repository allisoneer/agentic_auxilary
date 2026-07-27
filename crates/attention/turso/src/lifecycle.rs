use crate::Error;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LifecycleState {
    Closed = 0,
    Opening = 1,
    Open = 2,
    Draining = 3,
}

#[derive(Debug)]
pub struct Lifecycle {
    state: AtomicU8,
    active: AtomicUsize,
    idle: Notify,
}

impl Lifecycle {
    pub(crate) fn open() -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU8::new(LifecycleState::Open as u8),
            active: AtomicUsize::new(0),
            idle: Notify::new(),
        })
    }

    pub(crate) fn state(&self) -> LifecycleState {
        match self.state.load(Ordering::Acquire) {
            1 => LifecycleState::Opening,
            2 => LifecycleState::Open,
            3 => LifecycleState::Draining,
            _ => LifecycleState::Closed,
        }
    }

    pub(crate) fn acquire(self: &Arc<Self>) -> Result<LifecyclePermit, Error> {
        if self.state() != LifecycleState::Open {
            return Err(Error::Shutdown);
        }
        self.active.fetch_add(1, Ordering::AcqRel);
        if self.state() != LifecycleState::Open {
            self.release();
            return Err(Error::Shutdown);
        }
        Ok(LifecyclePermit {
            lifecycle: Arc::clone(self),
        })
    }

    pub(crate) async fn begin_drain(&self) -> Result<(), Error> {
        self.state
            .compare_exchange(
                LifecycleState::Open as u8,
                LifecycleState::Draining as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| Error::Lifecycle)?;
        while self.active.load(Ordering::Acquire) != 0 {
            self.idle.notified().await;
        }
        Ok(())
    }

    pub(crate) fn finish_close(&self) {
        self.state
            .store(LifecycleState::Closed as u8, Ordering::Release);
    }

    pub(crate) fn begin_open(&self) -> Result<(), Error> {
        self.state
            .compare_exchange(
                LifecycleState::Closed as u8,
                LifecycleState::Opening as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| Error::Lifecycle)
    }

    pub(crate) fn finish_open(&self, success: bool) {
        let state = if success {
            LifecycleState::Open
        } else {
            LifecycleState::Closed
        };
        self.state.store(state as u8, Ordering::Release);
    }

    fn release(&self) {
        if self.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.idle.notify_waiters();
        }
    }
}

#[derive(Debug)]
pub struct LifecyclePermit {
    lifecycle: Arc<Lifecycle>,
}

impl Drop for LifecyclePermit {
    fn drop(&mut self) {
        self.lifecycle.release();
    }
}
