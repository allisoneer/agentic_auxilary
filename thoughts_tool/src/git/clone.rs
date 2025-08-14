use anyhow::{Context, Result};
use colored::*;
use git2::{FetchOptions, Progress, RemoteCallbacks};
use std::path::{Path, PathBuf};

pub struct CloneOptions {
    pub url: String,
    pub target_path: PathBuf,
    pub shallow: bool,
    pub branch: Option<String>,
}

pub fn clone_repository(options: &CloneOptions) -> Result<()> {
    println!("{} {}", "Cloning".green(), options.url);
    println!("  to: {}", options.target_path.display());

    // Ensure parent directory exists
    if let Some(parent) = options.target_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create clone directory")?;
    }

    // Ensure target doesn't exist or is empty
    if options.target_path.exists() {
        let entries = std::fs::read_dir(&options.target_path)?;
        if entries.count() > 0 {
            anyhow::bail!(
                "Target directory is not empty: {}",
                options.target_path.display()
            );
        }
    }

    // Set up clone
    let mut builder = git2::build::RepoBuilder::new();
    let mut fetch_opts = FetchOptions::new();
    let mut callbacks = RemoteCallbacks::new();

    // Add SSH credential callback
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
                "id_ed25519", // Modern default (Ed25519)
                "id_rsa",     // Legacy default (RSA)
                "id_ecdsa",   // Less common (ECDSA)
            ];

            for key_name in &key_files {
                let private_key = ssh_dir.join(key_name);
                if private_key.exists() {
                    // Try without public key path first (often sufficient)
                    if let Ok(cred) = git2::Cred::ssh_key(
                        username,
                        None, // No public key path
                        private_key.as_path(),
                        None, // No passphrase support
                    ) {
                        return Ok(cred);
                    }

                    // If that fails, try with public key
                    let public_key = ssh_dir.join(format!("{key_name}.pub"));
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
                 Note: Passphrase-protected keys are not currently supported",
            ))
        } else if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            // Fall back to git credential helper for HTTPS
            git2::Cred::default()
        } else {
            git2::Cred::default()
        }
    });

    // Keep existing progress callback
    callbacks.transfer_progress(|stats: Progress| {
        let received = stats.received_objects();
        let total = stats.total_objects();

        if total > 0 {
            let percent = (received as f32 / total as f32) * 100.0;
            print!(
                "\r  {}: {}/{} objects ({:.1}%)",
                "Progress".cyan(),
                received,
                total,
                percent
            );
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
        true
    });

    fetch_opts.remote_callbacks(callbacks);
    builder.fetch_options(fetch_opts);

    if let Some(branch) = &options.branch {
        builder.branch(branch);
    }

    // Perform clone
    builder
        .clone(&options.url, &options.target_path)
        .context("Failed to clone repository")?;

    println!("\n✓ Clone completed successfully");
    Ok(())
}

pub fn is_valid_clone_target(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }

    if path.is_dir() {
        let entries = std::fs::read_dir(path)?;
        Ok(entries.count() == 0)
    } else {
        Ok(false)
    }
}
