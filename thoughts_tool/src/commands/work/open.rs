use anyhow::{Result, bail};
use colored::Colorize;
use std::env;
use std::path::PathBuf;
use std::process::Command;

use thoughts_tool::workspace::ensure_active_work;

#[derive(Debug, Clone, Copy)]
pub enum OpenSubdir {
    Base,
    Research,
    Plans,
    Artifacts,
}

pub async fn execute(subdir: OpenSubdir) -> Result<()> {
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let aw = ensure_active_work()?;

    let target: PathBuf = match subdir {
        OpenSubdir::Base => aw.base.clone(),
        OpenSubdir::Research => aw.research.clone(),
        OpenSubdir::Plans => aw.plans.clone(),
        OpenSubdir::Artifacts => aw.artifacts.clone(),
    };

    if !target.exists() {
        bail!("Target directory does not exist: {}", target.display());
    }

    let status = Command::new(&editor).arg(&target).status()?;
    if !status.success() {
        bail!("Editor exited with error");
    }

    println!("{} Opened {}", "✓".green(), target.display());
    Ok(())
}
