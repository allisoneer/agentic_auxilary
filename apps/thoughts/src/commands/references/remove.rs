use crate::config::{ReferenceEntry, RepoConfigManager};
use crate::git::utils::get_control_repo_root;
use anyhow::Result;
use colored::Colorize;
use thoughts_tool::config::repo_mapping_manager::parse_url_and_subpath;
use thoughts_tool::repo_identity::RepoIdentity;

/// How a removal was resolved.
#[derive(Debug, PartialEq, Eq)]
enum RemoveResolution {
    /// The input URL matched exactly as stored
    Exact,
    /// The input URL was not found exactly, but canonical identity matches were found
    CanonicalFallback,
}

/// Summary of a removal operation.
struct RemoveSummary {
    /// URLs that were removed
    removed: Vec<String>,
    /// How the removal was resolved
    resolution: RemoveResolution,
}

/// Remove references matching the input URL by canonical identity.
///
/// Returns a summary of what was removed and how the match was resolved.
fn remove_references_by_canonical(entries: &mut Vec<ReferenceEntry>, input: &str) -> RemoveSummary {
    let (input_base, _) = parse_url_and_subpath(input);
    let input_key = RepoIdentity::parse(&input_base)
        .ok()
        .map(|id| id.canonical_key());

    let mut removed = Vec::new();
    let mut saw_exact = false;

    entries.retain(|entry| {
        let remote = match entry {
            ReferenceEntry::Simple(u) => u,
            ReferenceEntry::WithMetadata(rm) => &rm.remote,
        };

        if remote == input {
            saw_exact = true;
        }

        let should_remove = if let Some(ref key) = input_key {
            let (remote_base, _) = parse_url_and_subpath(remote);
            RepoIdentity::parse(&remote_base)
                .ok()
                .map(|id| id.canonical_key())
                .as_ref()
                == Some(key)
        } else {
            // If input URL is not parseable, fall back to exact match
            remote == input
        };

        if should_remove {
            removed.push(remote.clone());
            false
        } else {
            true
        }
    });

    let resolution = if removed.is_empty() || saw_exact || input_key.is_none() {
        RemoveResolution::Exact
    } else {
        RemoveResolution::CanonicalFallback
    };

    RemoveSummary {
        removed,
        resolution,
    }
}

pub async fn execute(url: String) -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);

    let mut cfg = mgr.load_v2_or_bail()?;

    let summary = remove_references_by_canonical(&mut cfg.references, &url);

    if summary.removed.is_empty() {
        println!("{} Reference not found: {}", "✗".red(), url);
        anyhow::bail!("Reference not found");
    }

    let warnings = mgr.save_v2_validated(&cfg)?;
    for w in warnings {
        eprintln!("Warning: {}", w);
    }

    // Print what was removed
    for removed_url in &summary.removed {
        println!("{} Removed reference: {}", "✓".green(), removed_url);
    }

    // Print hint if we matched by canonical identity but not exact string
    if summary.resolution == RemoveResolution::CanonicalFallback {
        println!(
            "\n{} Hint: Removed '{}' (matched canonical identity for input '{}')",
            "ℹ".blue(),
            summary.removed[0],
            url
        );
    }

    println!(
        "\nNote: The cloned repository is not deleted. Use 'thoughts references doctor --fix' to clean up stale mappings in repos.json."
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_removes_all_canonical_matches() {
        let mut entries = vec![
            ReferenceEntry::Simple("git@github.com:org/repo.git".to_string()),
            ReferenceEntry::Simple("https://github.com/org/repo".to_string()),
        ];

        let summary = remove_references_by_canonical(&mut entries, "https://github.com/org/repo");
        assert_eq!(summary.removed.len(), 2);
        assert!(entries.is_empty());
        // exact match existed, so resolution is Exact
        assert_eq!(summary.resolution, RemoveResolution::Exact);
    }

    #[test]
    fn remove_reports_canonical_fallback_when_exact_missing() {
        let mut entries = vec![ReferenceEntry::Simple(
            "git@github.com:org/repo.git".to_string(),
        )];

        let summary = remove_references_by_canonical(&mut entries, "https://github.com/org/repo");
        assert_eq!(
            summary.removed,
            vec!["git@github.com:org/repo.git".to_string()]
        );
        assert_eq!(summary.resolution, RemoveResolution::CanonicalFallback);
    }

    #[test]
    fn remove_falls_back_to_exact_when_unparseable() {
        let mut entries = vec![ReferenceEntry::Simple("not-a-valid-url".to_string())];

        let summary = remove_references_by_canonical(&mut entries, "not-a-valid-url");
        assert_eq!(summary.removed, vec!["not-a-valid-url".to_string()]);
        assert_eq!(summary.resolution, RemoveResolution::Exact);
    }

    #[test]
    fn remove_ssh_with_https_input() {
        let mut entries = vec![ReferenceEntry::Simple(
            "git@github.com:user/lib.git".to_string(),
        )];

        let summary = remove_references_by_canonical(&mut entries, "https://github.com/user/lib");
        assert_eq!(
            summary.removed,
            vec!["git@github.com:user/lib.git".to_string()]
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn remove_https_with_ssh_input() {
        let mut entries = vec![ReferenceEntry::Simple(
            "https://github.com/user/lib".to_string(),
        )];

        let summary = remove_references_by_canonical(&mut entries, "git@github.com:user/lib.git");
        assert_eq!(
            summary.removed,
            vec!["https://github.com/user/lib".to_string()]
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn remove_preserves_unrelated_entries() {
        let mut entries = vec![
            ReferenceEntry::Simple("git@github.com:org/repo.git".to_string()),
            ReferenceEntry::Simple("https://github.com/other/project".to_string()),
        ];

        let summary = remove_references_by_canonical(&mut entries, "https://github.com/org/repo");
        assert_eq!(summary.removed.len(), 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            ReferenceEntry::Simple("https://github.com/other/project".to_string())
        );
    }
}
