use anyhow::Result;
use anyhow::bail;
use colored::Colorize;
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
    let argv = agentic_tools_utils::editor_argv()?;
    let aw = ensure_active_work()?;

    let target: PathBuf = match subdir {
        OpenSubdir::Base => aw.base,
        OpenSubdir::Research => aw.research,
        OpenSubdir::Plans => aw.plans,
        OpenSubdir::Artifacts => aw.artifacts,
    };

    if !target.exists() {
        bail!("Target directory does not exist: {}", target.display());
    }

    let status = Command::new(&argv.program)
        .args(&argv.args)
        .arg(&target)
        .status()?;
    if !status.success() {
        bail!("Editor exited with error");
    }

    println!("{} Opened {}", "✓".green(), target.display());
    Ok(())
}
