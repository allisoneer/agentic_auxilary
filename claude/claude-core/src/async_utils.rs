//! Async utilities

use futures::future::{AbortHandle, Abortable};
use std::sync::Arc;

/// Handle that aborts its future when dropped
#[derive(Clone)]
#[allow(dead_code)] // Field is used for its Drop implementation
pub struct AbortOnDropHandle(Arc<AbortHandleInner>);

struct AbortHandleInner(AbortHandle);

impl Drop for AbortHandleInner {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Create an abortable future with a handle that aborts on drop
pub fn abort_on_drop<F>(future: F) -> (Abortable<F>, AbortOnDropHandle) {
    let (handle, reg) = AbortHandle::new_pair();
    (
        Abortable::new(future, reg),
        AbortOnDropHandle(Arc::new(AbortHandleInner(handle))),
    )
}
