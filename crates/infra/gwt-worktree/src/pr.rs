use crate::error::Result;
use crate::types::BranchName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
    Unknown,
}

pub trait PullRequestLookup {
    fn lookup_pull_request_state(&self, branch: &BranchName) -> Result<PullRequestState>;
}
