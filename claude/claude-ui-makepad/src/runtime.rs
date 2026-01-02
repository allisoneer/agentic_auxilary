//! Tokio runtime management for Makepad integration

#![allow(dead_code)] // Runtime is used via crate::runtime::spawn

use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Get the global tokio runtime
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .thread_name("claude-ui-tokio")
            .build()
            .expect("Failed to create tokio runtime")
    })
}

/// Spawn a future on the global runtime
pub fn spawn<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    runtime().spawn(future);
}
