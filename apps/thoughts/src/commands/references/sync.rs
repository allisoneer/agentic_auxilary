use crate::config::validation::validate_reference_url;
use crate::config::{RepoConfigManager, RepoMappingManager};
use crate::git::clone::{CloneOptions, clone_repository};
use crate::git::pull::pull_ff_only;
use crate::git::utils::{get_control_repo_root, get_current_branch};
use anyhow::Result;
use colored::Colorize;
use thoughts_tool::config::repo_mapping_manager::UrlResolutionKind;
use thoughts_tool::repo_identity::RepoIdentity;

pub async fn execute(verbose: bool) -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);
    let mut mapping_mgr = RepoMappingManager::new()?;

    let ds = mgr.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;

    if ds.references.is_empty() {
        println!("No references configured.");
        return Ok(());
    }

    let mut cloned_count = 0;
    let mut updated_count = 0;
    let mut skipped_count = 0;
    let mut invalid_count = 0;

    for rm in &ds.references {
        let url = &rm.remote;

        if let Err(e) = validate_reference_url(url) {
            println!(
                "{} Skipping invalid reference: {}\n{}",
                "⚠".yellow(),
                url,
                e
            );
            invalid_count += 1;
            continue;
        }

        // Print canonical identity in verbose mode
        if verbose && let Ok(id) = RepoIdentity::parse(url) {
            let key = id.canonical_key();
            println!("Reference: {}", url);
            println!("  canonical: {}/{}/{}", key.host, key.org_path, key.repo);
        }

        // Use resolve_url_with_details for verbose output
        if let Some((resolved, _sub)) = mapping_mgr.resolve_url_with_details(url)? {
            let local_path = resolved.location.path.clone();

            if verbose {
                let resolution_str = match resolved.resolution {
                    UrlResolutionKind::Exact => "exact",
                    UrlResolutionKind::CanonicalFallback => "canonical-fallback",
                };
                println!(
                    "  mapping: {} ({}) -> {}",
                    resolved.matched_url,
                    resolution_str,
                    local_path.display()
                );
            }

            // Try fast-forward-only pull; skip detached head
            match get_current_branch(&local_path) {
                Ok(branch) if branch != "detached" => {
                    match pull_ff_only(&local_path, "origin", Some(&branch)) {
                        Ok(_) => {
                            if verbose {
                                println!("  action: updated (ff-only)");
                            } else {
                                println!("{} Updated {} (ff-only)", "↻".green(), url);
                            }
                            updated_count += 1;
                            let _ = mapping_mgr.update_sync_time(url);
                        }
                        Err(e) => {
                            if verbose {
                                println!("  action: FAILED - {}", e);
                            } else {
                                println!(
                                    "{} Failed to update {} (continuing): {}",
                                    "✗".red(),
                                    url,
                                    e
                                );
                            }
                            skipped_count += 1;
                        }
                    }
                }
                _ => {
                    if verbose {
                        println!("  action: skipped (detached HEAD)");
                    } else {
                        println!("{} Skipping update (detached HEAD): {}", "⚠".yellow(), url);
                    }
                    skipped_count += 1;
                }
            }
            continue;
        }

        // Not mapped locally - clone to default path
        let default_path = RepoMappingManager::get_default_clone_path(url)?;

        if verbose {
            println!("  mapping: (none)");
            println!("  action: cloning to {}", default_path.display());
        }

        match clone_repository(&CloneOptions {
            url: url.to_string(),
            target_path: default_path.clone(),
            branch: None,
        }) {
            Ok(_) => {
                // Add mapping
                mapping_mgr.add_mapping(url.to_string(), default_path, true)?;
                if !verbose {
                    println!("{} Cloned {}", "✓".green(), url);
                }
                cloned_count += 1;
                let _ = mapping_mgr.update_sync_time(url);
            }
            Err(e) => {
                if verbose {
                    println!("  action: FAILED - {}", e);
                } else {
                    println!("{} Failed to clone {}: {}", "✗".red(), url, e);
                }
                skipped_count += 1;
            }
        }
    }

    println!();
    println!(
        "Cloned: {}, Updated: {}, Skipped: {}, Invalid: {}",
        cloned_count, updated_count, skipped_count, invalid_count
    );

    if cloned_count + updated_count > 0 {
        println!("Run 'thoughts mount update' to mount the references.");
    }

    Ok(())
}
