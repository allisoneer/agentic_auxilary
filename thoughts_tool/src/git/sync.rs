use git2::{Repository, Signature, IndexAddOption, RemoteCallbacks, PushOptions, FetchOptions};
use anyhow::{Result, Context, bail};
use colored::*;
use std::path::Path;

pub struct GitSync {
    repo: Repository,
    subpath: Option<String>,
}

impl GitSync {
    pub fn new(repo_path: &Path, subpath: Option<String>) -> Result<Self> {
        let repo = Repository::open(repo_path)?;
        Ok(Self { repo, subpath })
    }
    
    pub async fn sync(&self, mount_name: &str) -> Result<()> {
        println!("  {} {}", "Syncing".cyan(), mount_name);
        
        // 1. Stage changes (respecting subpath)
        let changes_staged = self.stage_changes().await?;
        
        // 2. Commit if there are changes
        if changes_staged {
            self.commit(mount_name).await?;
            println!("    {} Committed changes", "✓".green());
        } else {
            println!("    {} No changes to commit", "○".dimmed());
        }
        
        // 3. Pull with rebase
        match self.pull_rebase().await {
            Ok(pulled) => {
                if pulled {
                    println!("    {} Pulled remote changes", "✓".green());
                }
            },
            Err(e) => {
                println!("    {} Pull failed: {}", "⚠".yellow(), e);
                // Continue anyway - will try to push local changes
            }
        }
        
        // 4. Push (non-fatal)
        match self.push().await {
            Ok(_) => println!("    {} Pushed to remote", "✓".green()),
            Err(e) => {
                println!("    {} Push failed: {}", "⚠".yellow(), e);
                println!("      {} Changes saved locally only", "Info".dimmed());
            }
        }
        
        Ok(())
    }
    
    async fn stage_changes(&self) -> Result<bool> {
        let mut index = self.repo.index()?;
        
        // Get the pathspec for staging
        let pathspecs: Vec<String> = if let Some(subpath) = &self.subpath {
            // Only stage files within subpath
            // Use glob pattern to match all files recursively
            vec![
                format!("{}/*", subpath),     // Files directly in subpath
                format!("{}/**/*", subpath),  // Files in subdirectories
            ]
        } else {
            // Stage all changes in repo
            vec![".".to_string()]
        };
        
        // Configure flags for proper subpath handling
        let flags = IndexAddOption::DEFAULT;
        
        // Track if we staged anything
        let mut staged_files = 0;
        
        // Stage new and modified files with callback to track what we're staging
        let cb = &mut |_path: &std::path::Path, _matched_spec: &[u8]| -> i32 {
            staged_files += 1;
            0  // Include this file
        };
        
        // Add all matching files
        index.add_all(
            pathspecs.iter(),
            flags,
            Some(cb as &mut git2::IndexMatchedPath)
        )?;
        
        // Update index to catch deletions in the pathspec
        index.update_all(pathspecs.iter(), None)?;
        
        index.write()?;
        
        // Check if we actually have changes to commit
        // Handle empty repo case where HEAD doesn't exist yet
        let diff = match self.repo.head() {
            Ok(head) => {
                let head_tree = self.repo.find_commit(head.target().unwrap())?.tree()?;
                self.repo.diff_tree_to_index(Some(&head_tree), Some(&index), None)?
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                // Empty repo - no HEAD yet, so everything in index is new
                self.repo.diff_tree_to_index(None, Some(&index), None)?
            }
            Err(e) => return Err(e.into()),
        };
        
        Ok(diff.stats()?.files_changed() > 0)
    }
    
    async fn commit(&self, mount_name: &str) -> Result<()> {
        let sig = Signature::now("thoughts-sync", "thoughts@sync.local")?;
        let tree_id = self.repo.index()?.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        
        // Create descriptive commit message
        let message = if let Some(subpath) = &self.subpath {
            format!("Auto-sync thoughts for {} (subpath: {})", mount_name, subpath)
        } else {
            format!("Auto-sync thoughts for {}", mount_name)
        };
        
        // Handle both initial commit and subsequent commits
        match self.repo.head() {
            Ok(head) => {
                // Normal commit with parent
                let parent = self.repo.find_commit(head.target().unwrap())?;
                self.repo.commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &message,
                    &tree,
                    &[&parent],
                )?;
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                // Initial commit - no parents
                self.repo.commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &message,
                    &tree,
                    &[],  // No parents for initial commit
                )?;
            }
            Err(e) => return Err(e.into()),
        }
        
        Ok(())
    }
    
    async fn pull_rebase(&self) -> Result<bool> {
        // Check if origin exists
        let mut remote = match self.repo.find_remote("origin") {
            Ok(remote) => remote,
            Err(e) if e.code() == git2::ErrorCode::NotFound => {
                // No origin remote - this is fine, just can't pull
                println!("    {} No remote 'origin' configured (local-only)", "Info".dimmed());
                return Ok(false);
            }
            Err(e) => return Err(e.into()),
        };
        
        let refs: Vec<String> = vec![];
        
        // Set up fetch options with authentication
        let mut fetch_options = FetchOptions::new();
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, allowed_types| {
            // Only handle SSH key authentication
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                let username = username_from_url.unwrap_or("git");
                
                // CRITICAL FIX: Only try SSH agent if SSH_AUTH_SOCK is set
                // This avoids the libssh2 bug where ssh_key_from_agent returns Ok
                // with invalid credentials when no agent is present
                // Bug ref: https://github.com/libssh2/libssh2/issues/659
                if std::env::var("SSH_AUTH_SOCK").is_ok() {
                    // Try SSH agent (might still fail due to libssh2 RSA-SHA2 bug)
                    if let Ok(cred) = git2::Cred::ssh_key_from_agent(username) {
                        return Ok(cred);
                    }
                    // If agent fails, fall through to key files
                }
                
                // Try SSH keys from disk (this is what actually works)
                let home = dirs::home_dir()
                    .ok_or_else(|| git2::Error::from_str("Cannot find home directory"))?;
                let ssh_dir = home.join(".ssh");
                
                // Try keys in order of preference
                let key_files = [
                    "id_ed25519",  // Modern default (Ed25519)
                    "id_rsa",      // Legacy default (RSA)
                    "id_ecdsa",    // Less common (ECDSA)
                ];
                
                for key_name in &key_files {
                    let private_key = ssh_dir.join(key_name);
                    if private_key.exists() {
                        // Try without public key path first (often sufficient)
                        if let Ok(cred) = git2::Cred::ssh_key(
                            username,
                            None,  // No public key path
                            private_key.as_path(),
                            None,  // No passphrase support
                        ) {
                            return Ok(cred);
                        }
                        
                        // If that fails, try with public key
                        let public_key = ssh_dir.join(format!("{}.pub", key_name));
                        if public_key.exists() {
                            if let Ok(cred) = git2::Cred::ssh_key(
                                username,
                                Some(public_key.as_path()),
                                private_key.as_path(),
                                None,
                            ) {
                                return Ok(cred);
                            }
                        }
                    }
                }
                
                Err(git2::Error::from_str(
                    "SSH authentication failed. No valid SSH keys found in ~/.ssh/\n\
                     Checked for: id_ed25519, id_rsa, id_ecdsa\n\
                     Note: Passphrase-protected keys are not currently supported"
                ))
            } else if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
                // Fall back to git credential helper for HTTPS
                git2::Cred::default()
            } else {
                git2::Cred::default()
            }
        });
        fetch_options.remote_callbacks(callbacks);
        
        // Fetch with authentication
        remote.fetch(&refs, Some(&mut fetch_options), None)?;
        
        // Get current branch
        let head = self.repo.head()?;
        let branch_name = head.shorthand().unwrap_or("main");
        
        // Try to find the upstream commit
        let upstream_oid = match self.repo.refname_to_id(&format!("refs/remotes/origin/{}", branch_name)) {
            Ok(oid) => oid,
            Err(_) => {
                // No upstream branch yet
                return Ok(false);
            }
        };
        
        let upstream_commit = self.repo.find_annotated_commit(upstream_oid)?;
        let head_commit = self.repo.find_annotated_commit(head.target().unwrap())?;
        
        // Check if we need to rebase
        let analysis = self.repo.merge_analysis(&[&upstream_commit])?;
        
        if analysis.0.is_up_to_date() {
            return Ok(false);
        }
        
        if analysis.0.is_fast_forward() {
            // Fast-forward
            let mut reference = self.repo.find_reference("HEAD")?;
            reference.set_target(upstream_oid, "Fast-forward")?;
            self.repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
            return Ok(true);
        }
        
        // Need to rebase
        let mut rebase = self.repo.rebase(
            Some(&head_commit),
            Some(&upstream_commit),
            None,
            None
        )?;
        
        while let Some(operation) = rebase.next() {
            if let Ok(_op) = operation {
                if self.repo.index()?.has_conflicts() {
                    // Resolve conflicts by preferring remote
                    self.resolve_conflicts_prefer_remote()?;
                }
                rebase.commit(None, &Signature::now("thoughts-sync", "thoughts@sync.local")?, None)?;
            }
        }
        
        rebase.finish(None)?;
        Ok(true)
    }
    
    async fn push(&self) -> Result<()> {
        let mut remote = match self.repo.find_remote("origin") {
            Ok(remote) => remote,
            Err(e) if e.code() == git2::ErrorCode::NotFound => {
                // No origin remote - can't push
                println!("    {} No remote 'origin' configured (local-only)", "Info".dimmed());
                return Ok(());  // Not an error, just skip push
            }
            Err(e) => return Err(e.into()),
        };
        
        let head = self.repo.head()?;
        let branch = head.shorthand().unwrap_or("main");
        
        // Configure push options
        let mut push_options = PushOptions::new();
        
        // Try to use credential helper
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, allowed_types| {
            // Only handle SSH key authentication
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                let username = username_from_url.unwrap_or("git");
                
                // CRITICAL FIX: Only try SSH agent if SSH_AUTH_SOCK is set
                // This avoids the libssh2 bug where ssh_key_from_agent returns Ok
                // with invalid credentials when no agent is present
                // Bug ref: https://github.com/libssh2/libssh2/issues/659
                if std::env::var("SSH_AUTH_SOCK").is_ok() {
                    // Try SSH agent (might still fail due to libssh2 RSA-SHA2 bug)
                    if let Ok(cred) = git2::Cred::ssh_key_from_agent(username) {
                        return Ok(cred);
                    }
                    // If agent fails, fall through to key files
                }
                
                // Try SSH keys from disk (this is what actually works)
                let home = dirs::home_dir()
                    .ok_or_else(|| git2::Error::from_str("Cannot find home directory"))?;
                let ssh_dir = home.join(".ssh");
                
                // Try keys in order of preference
                let key_files = [
                    "id_ed25519",  // Modern default (Ed25519)
                    "id_rsa",      // Legacy default (RSA)
                    "id_ecdsa",    // Less common (ECDSA)
                ];
                
                for key_name in &key_files {
                    let private_key = ssh_dir.join(key_name);
                    if private_key.exists() {
                        // Try without public key path first (often sufficient)
                        if let Ok(cred) = git2::Cred::ssh_key(
                            username,
                            None,  // No public key path
                            private_key.as_path(),
                            None,  // No passphrase support
                        ) {
                            return Ok(cred);
                        }
                        
                        // If that fails, try with public key
                        let public_key = ssh_dir.join(format!("{}.pub", key_name));
                        if public_key.exists() {
                            if let Ok(cred) = git2::Cred::ssh_key(
                                username,
                                Some(public_key.as_path()),
                                private_key.as_path(),
                                None,
                            ) {
                                return Ok(cred);
                            }
                        }
                    }
                }
                
                Err(git2::Error::from_str(
                    "SSH authentication failed. No valid SSH keys found in ~/.ssh/\n\
                     Checked for: id_ed25519, id_rsa, id_ecdsa\n\
                     Note: Passphrase-protected keys are not currently supported"
                ))
            } else if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
                // Fall back to git credential helper for HTTPS
                git2::Cred::default()
            } else {
                git2::Cred::default()
            }
        });
        push_options.remote_callbacks(callbacks);
        
        // Push
        remote.push(
            &[&format!("refs/heads/{0}:refs/heads/{0}", branch)],
            Some(&mut push_options)
        )?;
        
        Ok(())
    }
    
    fn resolve_conflicts_prefer_remote(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        let conflicts: Vec<_> = index.conflicts()?.collect::<Result<Vec<_>, _>>()?;
        
        for conflict in conflicts {
            // Prefer their version (remote)
            if let Some(their) = conflict.their {
                index.add(&their)?;
            } else if let Some(our) = conflict.our {
                // If no remote version, keep ours
                index.add(&our)?;
            }
        }
        
        index.write()?;
        Ok(())
    }
}