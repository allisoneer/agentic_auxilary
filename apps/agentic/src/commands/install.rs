use crate::commands::install_manifest::INSTALL_MANIFEST;
use anyhow::Context;
use anyhow::Result;
use atomicwrites::AtomicFile;
use atomicwrites::OverwriteBehavior;
use colored::Colorize;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

pub fn execute(path: Option<PathBuf>, force: bool) -> Result<()> {
    let start_dir = resolve_dir(path)?;
    let repo_root = find_git_root(&start_dir)?;

    preflight(&repo_root, force)?;

    if INSTALL_MANIFEST.last().map(|asset| asset.rel_path) != Some("opencode.json") {
        anyhow::bail!("internal error: INSTALL_MANIFEST must end with opencode.json");
    }

    for asset in INSTALL_MANIFEST {
        let dst = repo_root.join(asset.rel_path);
        ensure_parent_dir(&dst)?;
        write_atomic_str(&dst, asset.contents, force)?;
    }

    println!(
        "{} Installed {} managed file(s) into {}",
        "OK".green(),
        INSTALL_MANIFEST.len(),
        repo_root.display().to_string().cyan()
    );

    Ok(())
}

fn resolve_dir(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        None => std::env::current_dir().context("Failed to determine current directory"),
        Some(path) => {
            if !path.exists() {
                anyhow::bail!("--path does not exist: {}", path.display());
            }
            if !path.is_dir() {
                anyhow::bail!("--path is not a directory: {}", path.display());
            }
            Ok(path)
        }
    }
}

fn find_git_root(start: &Path) -> Result<PathBuf> {
    let mut current = start
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize {}", start.display()))?;

    loop {
        let marker = current.join(".git");
        if marker.exists() {
            return Ok(current);
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }

    anyhow::bail!("Not in a git repository. Run 'git init' first.")
}

fn preflight(repo_root: &Path, force: bool) -> Result<()> {
    let mut existing_conflicts = Vec::new();
    let mut fatal_conflicts = Vec::new();

    for asset in INSTALL_MANIFEST {
        ensure_safe_rel_path(asset.rel_path)?;

        let dst = repo_root.join(asset.rel_path);

        if let Some(parent) = dst.parent() {
            preflight_parent_dirs(repo_root, parent).with_context(|| {
                format!("Preflight failed for parent dirs of {}", asset.rel_path)
            })?;
        }

        if dst.exists() {
            let meta = std::fs::symlink_metadata(&dst).with_context(|| {
                format!("Failed to read metadata for managed path {}", dst.display())
            })?;

            if meta.file_type().is_symlink() {
                fatal_conflicts.push(format!("{} (refusing to write to symlink)", asset.rel_path));
                continue;
            }

            if meta.is_dir() {
                fatal_conflicts.push(format!(
                    "{} (expected file, found directory)",
                    asset.rel_path
                ));
                continue;
            }

            if !force {
                existing_conflicts.push(asset.rel_path.to_string());
            }
        }
    }

    if !fatal_conflicts.is_empty() {
        let details = fatal_conflicts
            .into_iter()
            .map(|path| format!("  - {path}"))
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!("Install cannot proceed due to path conflicts:\n{details}");
    }

    if !existing_conflicts.is_empty() {
        let details = existing_conflicts
            .into_iter()
            .map(|path| format!("  - {path}"))
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!(
            "Managed file(s) already exist:\n{details}\nRe-run with --force to overwrite managed files."
        );
    }

    Ok(())
}

fn ensure_safe_rel_path(rel_path: &str) -> Result<()> {
    let path = Path::new(rel_path);
    if path.is_absolute() {
        anyhow::bail!("internal error: manifest path is absolute: {rel_path}");
    }

    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            anyhow::bail!("internal error: manifest path contains '..': {rel_path}");
        }
    }

    Ok(())
}

fn preflight_parent_dirs(repo_root: &Path, parent: &Path) -> Result<()> {
    let rel_parent = parent.strip_prefix(repo_root).unwrap_or(parent);
    let mut current = repo_root.to_path_buf();

    for component in rel_parent.components() {
        current.push(component);
        if current.exists() {
            let meta = std::fs::symlink_metadata(&current)
                .with_context(|| format!("Failed to inspect {}", current.display()))?;
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "refusing to traverse symlink directory: {}",
                    current.display()
                );
            }
            if !meta.is_dir() {
                anyhow::bail!("expected directory but found file: {}", current.display());
            }
        }
    }

    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    Ok(())
}

fn write_atomic_str(path: &Path, contents: &str, force: bool) -> Result<()> {
    let overwrite_behavior = if force {
        OverwriteBehavior::AllowOverwrite
    } else {
        OverwriteBehavior::DisallowOverwrite
    };

    AtomicFile::new(path, overwrite_behavior)
        .write(|file| file.write_all(contents.as_bytes()))
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}
