//! Chat controller plugin system

use crate::state::{ChatState, ChatStateMutation};

/// Control flow for plugin handling
pub enum ChatControl {
    /// Continue processing
    Continue,
    /// Stop processing
    Stop,
}

/// Plugin trait for chat controller
pub trait ChatControllerPlugin: Send {
    /// Called when a mutation is about to be applied
    fn on_state_mutation(&mut self, _mutation: &ChatStateMutation, _state: &ChatState) {}
    /// Called after mutations have been applied
    fn on_state_ready(&mut self, _state: &ChatState, _mutations: &[ChatStateMutation]) {}
    /// Called when a task is started
    fn on_task(&mut self, _task: &str) -> ChatControl {
        ChatControl::Continue
    }
}

/// Unique plugin identifier
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PluginId(u64);

impl PluginId {
    /// Generate a new unique plugin ID
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for PluginId {
    fn default() -> Self {
        Self::new()
    }
}
