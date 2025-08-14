use anyhow::Result;
use std::fs;
use std::path::Path;
use tracing::info;

/// Ensures a gitignore entry exists in the repository's .gitignore file
pub fn ensure_gitignore_entry(
    repo_path: &Path,
    entry: &str,
    comment: Option<&str>,
) -> Result<bool> {
    let gitignore_path = repo_path.join(".gitignore");

    // Check if .gitignore exists
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)?;

        // Check if entry is already ignored
        let entry_no_slash = entry.trim_start_matches('/');
        let has_entry = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == entry
                || trimmed == entry_no_slash
                || trimmed == format!("{entry_no_slash}/")
        });

        if !has_entry {
            // Append to .gitignore
            let mut new_content = content;
            if !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            if let Some(comment_text) = comment {
                new_content.push_str(&format!("\n# {comment_text}\n"));
            }
            new_content.push_str(entry);
            new_content.push('\n');

            fs::write(&gitignore_path, new_content)?;
            info!("Added {} to .gitignore", entry);
            Ok(true)
        } else {
            Ok(false)
        }
    } else {
        // Create new .gitignore
        let mut content = String::new();
        if let Some(comment_text) = comment {
            content.push_str(&format!("# {comment_text}\n"));
        }
        content.push_str(&format!("{entry}\n"));

        fs::write(&gitignore_path, content)?;
        info!("Created .gitignore with {} entry", entry);
        Ok(true)
    }
}
