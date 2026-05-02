//! Tool execution context.

use crate::ToolError;
use std::future::Future;
use tokio_util::sync::CancellationToken;
use tokio_util::sync::WaitForCancellationFutureOwned;

/// Context passed to tool executions.
///
/// MCP-backed tool calls receive a request-scoped cancellation token through this
/// context. Non-MCP callers that construct [`ToolContext::default`] or
/// [`ToolContext::new`] receive a never-cancelled context so existing direct,
/// native, and NAPI entrypoints remain source-compatible.
///
/// Tool authors should prefer these helpers instead of wiring cancellation by hand:
/// - [`ToolContext::run_cancellable`] for a single async operation that should abort
///   promptly when the request is cancelled.
/// - [`ToolContext::cancelled`] when cancellation must participate in a larger
///   `tokio::select!`.
/// - [`ToolContext::is_cancelled`] for quick boundary checks before starting more work.
/// - [`ToolContext::cancellation_token`] when an owned token clone must cross `.await`
///   boundaries inside a `BoxFuture<'static>` implementation.
///
/// Cancellation maps to [`ToolError::Cancelled`]. Returning that error means the tool
/// stopped because the caller cancelled the request, not because the tool failed.
///
/// For subprocess-managing tools, request cancellation should trigger explicit cleanup
/// before returning. Dropping a future remains a backstop, not the primary cooperative
/// cleanup path.
#[derive(Clone, Debug)]
pub struct ToolContext {
    cancel: CancellationToken,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self::with_cancel(CancellationToken::new())
    }
}

impl ToolContext {
    /// Create a new default context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a context backed by the supplied cancellation token.
    pub fn with_cancel(cancel: CancellationToken) -> Self {
        Self { cancel }
    }

    /// Clone the request cancellation token for use across `.await` boundaries.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Return an owned future that resolves when the request is cancelled.
    pub fn cancelled(&self) -> WaitForCancellationFutureOwned {
        self.cancel.clone().cancelled_owned()
    }

    /// Check whether the request has already been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Run an async operation that should stop promptly on request cancellation.
    pub async fn run_cancellable<F, T, E>(&self, fut: F) -> Result<T, ToolError>
    where
        F: Future<Output = Result<T, E>>,
        E: Into<ToolError>,
    {
        tokio::select! {
            _ = self.cancelled() => {
                tracing::info!("tool request cancelled during run_cancellable");
                Err(ToolError::cancelled(None))
            }
            result = fut => result.map_err(Into::into),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;
    use tokio::time::sleep;
    use tokio::time::timeout;

    #[tokio::test]
    async fn default_context_is_never_cancelled() {
        let ctx = ToolContext::default();

        assert!(!ctx.is_cancelled());
        assert!(
            timeout(Duration::from_millis(25), ctx.cancelled())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn with_cancel_propagates_cancellation() {
        let cancel = CancellationToken::new();
        let ctx = ToolContext::with_cancel(cancel.clone());

        cancel.cancel();
        ctx.cancelled().await;

        assert!(ctx.is_cancelled());
        assert!(ctx.cancellation_token().is_cancelled());
    }

    #[tokio::test]
    async fn run_cancellable_returns_inner_success() {
        let ctx = ToolContext::default();

        let result = ctx
            .run_cancellable(async { Ok::<_, ToolError>("done") })
            .await;

        assert!(matches!(result, Ok("done")));
    }

    #[tokio::test]
    async fn run_cancellable_returns_cancelled_when_request_is_cancelled() {
        let cancel = CancellationToken::new();
        let ctx = ToolContext::with_cancel(cancel.clone());

        let canceller = tokio::spawn(async move {
            sleep(Duration::from_millis(25)).await;
            cancel.cancel();
        });

        let result = ctx
            .run_cancellable(async {
                sleep(Duration::from_secs(5)).await;
                Ok::<(), ToolError>(())
            })
            .await;

        let join_result = canceller.await;
        assert!(join_result.is_ok());
        assert!(matches!(result, Err(ToolError::Cancelled { reason: None })));
    }
}
