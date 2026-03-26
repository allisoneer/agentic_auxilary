//! MCP tool implementations for review tools.

use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use futures::future::BoxFuture;
use std::sync::Arc;

use crate::ReviewTools;
use crate::types::{
    ReviewDiffPageInput, ReviewDiffPageOutput, ReviewDiffSnapshotInput, ReviewDiffSnapshotOutput,
    ReviewRunInput, ReviewRunOutput,
};

/// Tool for generating a paginated git diff snapshot.
#[derive(Clone)]
pub struct DiffSnapshotTool {
    svc: Arc<ReviewTools>,
}

impl Tool for DiffSnapshotTool {
    type Input = ReviewDiffSnapshotInput;
    type Output = ReviewDiffSnapshotOutput;

    const NAME: &'static str = "diff_snapshot";
    const DESCRIPTION: &'static str = "Generate a paginated git diff snapshot (pure git2), cache it server-side, and return a handle. \
         Use 'mode=staged' for staged-only changes, or 'mode=default' for working tree + staged vs merge-base.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let svc = Arc::clone(&self.svc);
        Box::pin(async move { svc.diff_snapshot(input).await })
    }
}

/// Tool for fetching a specific page of a cached diff snapshot.
#[derive(Clone)]
pub struct DiffPageTool {
    svc: Arc<ReviewTools>,
}

impl Tool for DiffPageTool {
    type Input = ReviewDiffPageInput;
    type Output = ReviewDiffPageOutput;

    const NAME: &'static str = "diff_page";
    const DESCRIPTION: &'static str = "Fetch one page (1-based) of a cached diff snapshot by handle. \
         Returns the diff content for that page along with metadata.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let svc = Arc::clone(&self.svc);
        Box::pin(async move { svc.diff_page(input).await })
    }
}

/// Tool for running a lens-based code review.
#[derive(Clone)]
pub struct RunTool {
    svc: Arc<ReviewTools>,
}

impl Tool for RunTool {
    type Input = ReviewRunInput;
    type Output = ReviewRunOutput;

    const NAME: &'static str = "run";
    const DESCRIPTION: &'static str = "Run a lens-based adversarial code review over a cached diff snapshot. \
         The diff is embedded in the reviewer prompt (fileless). \
         Returns a validated ReviewReport with findings and verdict.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let svc = Arc::clone(&self.svc);
        Box::pin(async move { svc.review_run(input).await })
    }
}

/// Build a tool registry containing all review tools.
pub fn build_registry(svc: Arc<ReviewTools>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<DiffSnapshotTool, ()>(DiffSnapshotTool {
            svc: Arc::clone(&svc),
        })
        .register::<DiffPageTool, ()>(DiffPageTool {
            svc: Arc::clone(&svc),
        })
        .register::<RunTool, ()>(RunTool { svc })
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_tools_core::Tool;

    #[test]
    fn diff_snapshot_tool_name() {
        assert_eq!(DiffSnapshotTool::NAME, "diff_snapshot");
    }

    #[test]
    fn diff_page_tool_name() {
        assert_eq!(DiffPageTool::NAME, "diff_page");
    }

    #[test]
    fn run_tool_name() {
        assert_eq!(RunTool::NAME, "run");
    }

    #[test]
    fn build_registry_contains_three_tools() {
        let svc = Arc::new(ReviewTools::new());
        let registry = build_registry(svc);
        assert_eq!(registry.len(), 3);
        assert!(registry.contains("diff_snapshot"));
        assert!(registry.contains("diff_page"));
        assert!(registry.contains("run"));
    }
}
