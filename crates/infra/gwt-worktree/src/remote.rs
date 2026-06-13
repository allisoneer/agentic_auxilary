use crate::error::Error;
use crate::error::Result;
use crate::types::BranchName;
use git2::BranchType;
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

pub(crate) fn resolve_remote_for_branch_deletion(
    repo: &Repository,
    branch: &BranchName,
) -> Result<String> {
    if let Some(remote) = upstream_remote_name(repo, branch)? {
        ensure_remote_exists(repo, branch, &remote)?;
        return Ok(remote);
    }

    let origin = String::from("origin");
    if has_remote(repo, &origin)? {
        return Ok(origin);
    }

    Err(Error::RemoteDeleteRemoteUnresolved {
        branch: branch.to_string(),
    })
}

fn upstream_remote_name(repo: &Repository, branch: &BranchName) -> Result<Option<String>> {
    let branch = match repo.find_branch(branch.as_str(), BranchType::Local) {
        Ok(branch) => branch,
        Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let upstream = match branch.upstream() {
        Ok(upstream) => upstream,
        Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some(name) = upstream.name()? else {
        return Ok(None);
    };
    let Some((remote, _)) = name.split_once('/') else {
        return Ok(None);
    };
    if remote == "." {
        Ok(None)
    } else {
        Ok(Some(remote.to_owned()))
    }
}

fn ensure_remote_exists(repo: &Repository, branch: &BranchName, remote: &str) -> Result<()> {
    if has_remote(repo, remote)? {
        Ok(())
    } else {
        Err(Error::RemoteDeleteRemoteMissing {
            branch: branch.to_string(),
            remote: remote.to_owned(),
        })
    }
}

fn has_remote(repo: &Repository, remote: &str) -> Result<bool> {
    let remotes = repo.remotes()?;
    Ok(remotes.iter().flatten().any(|name| name == remote))
}
