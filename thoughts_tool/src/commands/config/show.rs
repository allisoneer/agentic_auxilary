use anyhow::Result;
use colored::Colorize;

use crate::config::{ReferenceEntry, RepoConfigManager};
use crate::git::utils::get_control_repo_root;

pub async fn execute(json: bool) -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);

    match mgr.peek_config_version()? {
        None => {
            println!("No repository configuration found");
            println!("Run {} to initialize", "thoughts init".cyan());
        }
        Some(v) if v == "1.0" => {
            let cfg = mgr.load()?.unwrap();
            if json {
                println!("{}", serde_json::to_string_pretty(&cfg)?);
            } else {
                println!("{}", "Repository Configuration (v1)".bold());
                println!();
                println!("version: {}", cfg.version);
                println!("repository mount dir: {}", cfg.mount_dirs.repository);
                println!("requires ({}):", cfg.requires.len());
                for r in cfg.requires {
                    println!("  - {} -> {}", r.mount_path, r.remote);
                }
            }
        }
        Some(_) => {
            let cfg = mgr.load_v2_or_bail()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&cfg)?);
            } else {
                println!("{}", "Repository Configuration (v2)".bold());
                println!();
                println!("version: {}", cfg.version);
                println!(
                    "mount_dirs: thoughts='{}', context='{}', references='{}'",
                    cfg.mount_dirs.thoughts, cfg.mount_dirs.context, cfg.mount_dirs.references
                );
                if let Some(tm) = &cfg.thoughts_mount {
                    println!("thoughts_mount: {} (sync: {:?})", tm.remote, tm.sync);
                }
                println!("context_mounts ({}):", cfg.context_mounts.len());
                for m in &cfg.context_mounts {
                    println!("  - {} -> {} (sync: {:?})", m.mount_path, m.remote, m.sync);
                }
                println!("references ({}):", cfg.references.len());
                for r in &cfg.references {
                    match r {
                        ReferenceEntry::Simple(u) => println!("  - {}", u),
                        ReferenceEntry::WithMetadata(rm) => println!(
                            "  - {} ({})",
                            rm.remote,
                            rm.description.as_deref().unwrap_or("no description")
                        ),
                    }
                }
            }
        }
    }

    Ok(())
}
