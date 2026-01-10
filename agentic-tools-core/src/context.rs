//! Tool execution context.

/// Context passed to tool executions.
///
/// This struct is extensible for future needs like:
/// - Cancellation tokens
/// - Workspace/environment info
/// - Tracing/spans
/// - Request metadata
#[derive(Clone, Default, Debug)]
pub struct ToolContext {
    // Extensible: add cancellation, workspace, env, tracing, etc.
    _private: (),
}

impl ToolContext {
    /// Create a new default context.
    pub fn new() -> Self {
        Self::default()
    }
}
