use anyhow::Result;
use colored::Colorize;

pub async fn execute() -> Result<()> {
    println!(
        "{} active mounts to match configuration...",
        "Updating".green()
    );

    crate::mount::auto_mount::update_active_mounts().await?;

    println!("\n{} Mount update complete", "✓".green());
    println!("Run {} to see current status", "thoughts status".cyan());

    Ok(())
}
