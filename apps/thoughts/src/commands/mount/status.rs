use anyhow::Result;
use colored::Colorize;

use crate::mount::get_mount_manager;
use crate::platform::detect_platform;

/// Execute the mount status command, showing all active mounts
pub async fn execute() -> Result<()> {
    let platform = detect_platform()?;
    let mgr = get_mount_manager(&platform)?;
    let mounts = mgr.list_mounts().await?;

    if mounts.is_empty() {
        println!("{}", "No active mounts".dimmed());
        return Ok(());
    }

    println!("{}", "Active Mounts:".bold());
    for m in mounts {
        let srcs = m
            .sources
            .iter()
            .map(|s| s.display().to_string())
            .collect::<Vec<_>>()
            .join(" : ");
        println!(
            " - {} ← {} [{}]",
            m.target.display().to_string().green(),
            srcs,
            m.fs_type
        );
    }
    Ok(())
}
