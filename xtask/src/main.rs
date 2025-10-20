use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

pub mod marker;

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "Repo maintenance tasks")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Sync README dependency versions inside autodeps markers
    ReadmeSync {
        /// Path to the README (defaults to repo root README.md)
        #[arg(long, default_value = "README.md")]
        path: PathBuf,
        /// Dry-run: print diffable output to stdout but don't write
        #[arg(long)]
        dry_run: bool,
    },
}

fn strict_mode() -> bool {
    matches!(std::env::var("AUTODEPS_STRICT"), Ok(v) if v == "1" || v.eq_ignore_ascii_case("true"))
}

fn main() -> Result<()> {
    let args = Args::parse();
    match args.cmd {
        Cmd::ReadmeSync { path, dry_run } => readme_sync(path, dry_run),
    }
}

fn readme_sync(path: PathBuf, dry_run: bool) -> Result<()> {
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to run `cargo metadata`")?;
    let strict = strict_mode();
    let input =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;

    let (output, changed) = marker::apply_autodeps_markers(
        &input,
        &marker::RenderContext {
            metadata: &metadata,
            strict,
        },
    )?;

    if !changed {
        eprintln!("[autodeps] No changes needed for {}", path.display());
        return Ok(());
    }

    if dry_run {
        println!("{output}");
    } else {
        fs::write(&path, output).with_context(|| format!("Failed to write {}", path.display()))?;
        eprintln!("[autodeps] Updated {}", path.display());
    }

    Ok(())
}
