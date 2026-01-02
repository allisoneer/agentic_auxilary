use anyhow::Result;
use colored::Colorize;
use thoughts_tool::workspace::ensure_active_work;

pub async fn execute() -> Result<()> {
    let aw = ensure_active_work()?;
    println!("{} Initialized work at: {}", "✓".green(), aw.base.display());
    println!("  Branch: {}", aw.dir_name);
    println!("  Structure:");
    println!("    - research/");
    println!("    - plans/");
    println!("    - artifacts/");
    Ok(())
}
