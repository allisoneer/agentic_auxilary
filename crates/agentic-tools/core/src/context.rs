//! Tool execution context.

pub use tokio_util::sync::CancellationToken;

/// Context passed to tool executions.
///
/// This struct carries cooperative cancellation and is extensible for future needs like:
/// - Workspace/environment info
/// - Tracing/spans
/// - Request metadata
#[derive(Clone, Debug)]
pub struct ToolContext {
    cancellation_token: CancellationToken,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
        }
    }
}

impl ToolContext {
    /// Create a new default context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a context with an existing cancellation token.
    pub fn with_cancellation_token(cancellation_token: CancellationToken) -> Self {
        Self { cancellation_token }
    }

    /// Return the cancellation token for this tool execution.
    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }

    /// Return true when this tool execution has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    /// Wait until this tool execution is cancelled.
    pub async fn cancelled(&self) {
        self.cancellation_token.cancelled().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn cancellation_token_is_shared_with_context() {
        let token = CancellationToken::new();
        let ctx = ToolContext::with_cancellation_token(token.clone());

        assert!(!ctx.is_cancelled());

        token.cancel();

        assert!(ctx.is_cancelled());
    }

    #[tokio::test]
    async fn cancelled_future_observes_cancellation() {
        let token = CancellationToken::new();
        let ctx = ToolContext::with_cancellation_token(token.clone());

        token.cancel();

        ctx.cancelled().await;
    }
}
