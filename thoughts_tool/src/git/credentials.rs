use git2::{Cred, CredentialType, Error, RemoteCallbacks};
use std::path::{Path, PathBuf};

/// Configure default git credentials on the provided RemoteCallbacks:
/// - SSH: agent if SSH_AUTH_SOCK set, else try ~/.ssh/id_ed25519, id_rsa, id_ecdsa
/// - HTTPS: fall back to Cred::default() (system credential helpers)
pub fn configure_default_git_credentials(callbacks: &mut RemoteCallbacks<'_>) {
    callbacks.credentials(|url, username_from_url, allowed| {
        ssh_credentials_cb(url, username_from_url, allowed)
    });
}

/// Shared credential callback implementing SSH and HTTPS behavior.
/// Preserves the SSH_AUTH_SOCK workaround for libssh2 bug #659.
pub fn ssh_credentials_cb(
    _url: &str,
    username_from_url: Option<&str>,
    allowed: CredentialType,
) -> Result<Cred, Error> {
    // Phase 1 of SSH auth (libgit2): if requested, provide the username first
    if allowed.contains(CredentialType::USERNAME) {
        return Cred::username(default_username(username_from_url));
    }

    if allowed.contains(CredentialType::SSH_KEY) {
        let username = default_username(username_from_url);

        // CRITICAL FIX: Only try SSH agent if SSH_AUTH_SOCK is set
        // Avoids libssh2 issue #659 (false Ok when no agent)
        if should_try_ssh_agent()
            && let Ok(cred) = Cred::ssh_key_from_agent(username)
        {
            return Ok(cred);
        }
        // Fall through to disk keys if agent fails

        // Try SSH keys from disk in order of preference
        let home = dirs::home_dir().ok_or_else(|| Error::from_str("Cannot find home directory"))?;
        let candidates = candidate_private_keys(&home);

        for private_key in candidates {
            if !private_key.exists() {
                continue;
            }

            // Try without public key path first
            if let Ok(cred) = Cred::ssh_key(username, None, private_key.as_path(), None) {
                return Ok(cred);
            }

            // If that fails, try with public key
            let mut public_key = private_key.clone();
            public_key.set_extension("pub"); // id_ed25519 -> id_ed25519.pub
            if public_key.exists()
                && let Ok(cred) = Cred::ssh_key(
                    username,
                    Some(public_key.as_path()),
                    private_key.as_path(),
                    None,
                )
            {
                return Ok(cred);
            }
        }

        return Err(Error::from_str(
            "SSH authentication failed. No valid SSH keys found in ~/.ssh/\n\
             Checked for: id_ed25519, id_rsa, id_ecdsa\n\
             Note: Passphrase-protected keys are not currently supported",
        ));
    }

    if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
        // HTTPS path: use system credential helpers
        return Cred::default();
    }

    Cred::default()
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

        // Create empty key files in order
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

        // Only RSA exists
        File::create(ssh_dir.join("id_rsa")).unwrap();

        let candidates = candidate_private_keys(home);
        // Function returns all paths in fixed order; existence checked by caller
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
    fn test_username_credential_with_provided_username() {
        let allowed = CredentialType::USERNAME;
        let res = ssh_credentials_cb("ssh://example.com/org/repo", Some("alice"), allowed);
        assert!(res.is_ok(), "Expected USERNAME credential to be handled");
    }

    #[test]
    fn test_username_credential_defaults_to_git() {
        let allowed = CredentialType::USERNAME;
        let res = ssh_credentials_cb("ssh://example.com/org/repo", None, allowed);
        assert!(
            res.is_ok(),
            "Expected USERNAME credential to default to 'git'"
        );
    }
}
