use git2::{Cred, CredentialType, Error, RemoteCallbacks};
use std::path::{Path, PathBuf};
use tracing::{debug, error};

// Maximum number of real credential attempts we will provide to libgit2 in one operation.
// Agent (1) + up to 3 disk keys, plus buffer for re-asks → 12 keeps us safe without being too low.
const MAX_CRED_ATTEMPTS: u8 = 12;

/// Configure default git credentials on the provided RemoteCallbacks:
/// - SSH: try agent exactly once (if SSH_AUTH_SOCK set), then ~/.ssh/id_ed25519, id_rsa, id_ecdsa
/// - HTTPS: fall back to Cred::default() (system credential helpers)
pub fn configure_default_git_credentials(callbacks: &mut RemoteCallbacks<'_>) {
    // Stateful variables captured by the FnMut closure
    let mut attempted_agent = false;
    let mut next_disk_idx: usize = 0;
    let mut attempts_total: u8 = 0;

    callbacks.credentials(move |url, username_from_url, allowed| {
        let allowed_username = allowed.contains(CredentialType::USERNAME);
        let allowed_ssh = allowed.contains(CredentialType::SSH_KEY);
        let allowed_https = allowed.contains(CredentialType::USER_PASS_PLAINTEXT);

        // 1) USERNAME phase
        if allowed_username {
            let user = default_username(username_from_url);
            debug!(%url, username = user, "git credentials: providing USERNAME");
            return Cred::username(user);
        }

        // 2) HTTPS path
        if allowed_https && !allowed_ssh {
            debug!(%url, "git credentials: HTTPS default credential helper");
            return Cred::default();
        }

        // 3) SSH path
        if !allowed_ssh {
            // Unexpected combination: fallback safely
            debug!(%url, ?allowed, "git credentials: unexpected allowed types; fallback to default()");
            return Cred::default();
        }

        if attempts_total >= MAX_CRED_ATTEMPTS {
            error!(
                %url,
                attempts_total,
                "git credentials: safety fuse tripped"
            );
            return Err(Error::from_str(
                "SSH authentication failed: safety fuse triggered after 12 attempts. \
This usually indicates your SSH agent is repeatedly offering unsupported FIDO/sk-* keys \
(e.g., 1Password) to libssh2. Ensure an Ed25519 or RSA key is available in your agent or \
on disk (~/.ssh/id_ed25519 or id_rsa), or use an HTTPS remote."
            ));
        }

        let username = default_username(username_from_url);

        // 3a) Try SSH agent exactly once (if available)
        if !attempted_agent {
            attempted_agent = true;
            if should_try_ssh_agent() {
                attempts_total = attempts_total.saturating_add(1);
                debug!(%url, username, "git credentials: trying SSH agent (once)");
                return Cred::ssh_key_from_agent(username);
            } else {
                debug!(%url, "git credentials: SSH_AUTH_SOCK not set; skipping agent");
            }
        }

        // 3b) Try disk keys sequentially, each at most once per operation
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => {
                error!(%url, "git credentials: cannot determine home directory for ~/.ssh");
                return Err(Error::from_str("Cannot find home directory for ~/.ssh keys"));
            }
        };
        let candidates = candidate_private_keys(&home);

        // Iterate from where we left off; try to construct a usable credential
        while next_disk_idx < candidates.len() {
            let private_key = candidates[next_disk_idx].clone();
            next_disk_idx += 1;

            if !private_key.exists() {
                debug!(key = %private_key.display(), "git credentials: disk key missing; skipping");
                continue;
            }

            debug!(key = %private_key.display(), "git credentials: trying disk private key");
            // Try without public key first
            match Cred::ssh_key(username, None, private_key.as_path(), None) {
                Ok(cred) => {
                    attempts_total = attempts_total.saturating_add(1);
                    return Ok(cred);
                }
                Err(e1) => {
                    // If that fails, try with a .pub key if present
                    let mut public_key = private_key.clone();
                    public_key.set_extension("pub");
                    if public_key.exists() {
                        match Cred::ssh_key(
                            username,
                            Some(public_key.as_path()),
                            private_key.as_path(),
                            None,
                        ) {
                            Ok(cred) => {
                                attempts_total = attempts_total.saturating_add(1);
                                return Ok(cred);
                            }
                            Err(e2) => {
                                debug!(
                                    private = %private_key.display(),
                                    public = %public_key.display(),
                                    "git credentials: disk key failed without and with pub: {e1}; {e2}"
                                );
                            }
                        }
                    } else {
                        debug!(
                            private = %private_key.display(),
                            "git credentials: disk key failed without pub: {e1}; no .pub found"
                        );
                    }
                }
            }
        }

        error!(
            %url,
            "SSH authentication failed: exhausted SSH agent and disk keys"
        );
        Err(Error::from_str(
            "SSH authentication failed. Tried SSH agent once and keys in ~/.ssh: id_ed25519, id_rsa, id_ecdsa. \
This can happen if your agent only advertises unsupported FIDO/sk-* keys (e.g., 1Password) which libssh2 cannot use. \
Ensure an Ed25519 or RSA key is available in your agent or on disk, or switch the remote to HTTPS."
        ))
    });
}

// ===== Private helpers (unit tested) =====

fn should_try_ssh_agent() -> bool {
    std::env::var("SSH_AUTH_SOCK").is_ok()
}

fn default_username(username_from_url: Option<&str>) -> &str {
    username_from_url.unwrap_or("git")
}

fn candidate_private_keys(home: &Path) -> Vec<PathBuf> {
    let ssh_dir = home.join(".ssh");
    // Fixed order of preference
    let names = ["id_ed25519", "id_rsa", "id_ecdsa"];
    names.iter().map(|n| ssh_dir.join(n)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs::{self, File};
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_should_try_ssh_agent_with_env_var() {
        unsafe {
            std::env::set_var("SSH_AUTH_SOCK", "/tmp/mock-agent");
        }
        assert!(should_try_ssh_agent());
        unsafe {
            std::env::remove_var("SSH_AUTH_SOCK");
        }
    }

    #[test]
    #[serial]
    fn test_should_try_ssh_agent_without_env_var() {
        unsafe {
            std::env::remove_var("SSH_AUTH_SOCK");
        }
        assert!(!should_try_ssh_agent());
    }

    #[test]
    fn test_candidate_private_keys_order() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();

        let ssh_dir = home.join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();

        File::create(ssh_dir.join("id_ed25519")).unwrap();
        File::create(ssh_dir.join("id_rsa")).unwrap();
        File::create(ssh_dir.join("id_ecdsa")).unwrap();

        let candidates = candidate_private_keys(home);
        assert_eq!(candidates[0].file_name().unwrap(), "id_ed25519");
        assert_eq!(candidates[1].file_name().unwrap(), "id_rsa");
        assert_eq!(candidates[2].file_name().unwrap(), "id_ecdsa");
    }

    #[test]
    fn test_candidate_private_keys_partial() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();

        let ssh_dir = home.join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();

        File::create(ssh_dir.join("id_rsa")).unwrap();

        let candidates = candidate_private_keys(home);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[1].file_name().unwrap(), "id_rsa");
    }

    #[test]
    fn test_default_username_with_value() {
        assert_eq!(default_username(Some("alice")), "alice");
    }

    #[test]
    fn test_default_username_defaults_to_git() {
        assert_eq!(default_username(None), "git");
    }

    #[test]
    fn test_disk_key_sequence_skips_missing_and_advances_index() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let ssh_dir = home.join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();

        File::create(ssh_dir.join("id_rsa")).unwrap();
        File::create(ssh_dir.join("id_ecdsa")).unwrap();

        let candidates = candidate_private_keys(home);
        assert_eq!(candidates[0].file_name().unwrap(), "id_ed25519");
        assert!(!candidates[0].exists(), "ed25519 should be missing");
        assert!(candidates[1].exists(), "rsa should exist");
        assert!(candidates[2].exists(), "ecdsa should exist");
    }

    #[test]
    fn test_safety_fuse_constant_bounds() {
        assert!(MAX_CRED_ATTEMPTS >= 10 && MAX_CRED_ATTEMPTS <= 16);
        assert_eq!(MAX_CRED_ATTEMPTS, 12);
    }
}
