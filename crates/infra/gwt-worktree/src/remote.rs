use crate::error::Result;
use crate::types::BranchName;
use git2::Repository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBranchTarget {
    pub remote: String,
    pub refname: String,
    pub commit_oid: String,
}

pub trait RemoteRefresher {
    fn refresh(&self, repo: &Repository) -> Result<()>;
    fn resolve_branch_target(
        &self,
        repo: &Repository,
        branch: &BranchName,
    ) -> Result<Option<RemoteBranchTarget>>;
}

pub trait RemoteBranchDeleter {
    fn delete_remote_branch(
        &self,
        repo: &Repository,
        remote: &str,
        branch: &BranchName,
    ) -> Result<()>;
}
