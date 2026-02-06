pub mod claude_settings;
pub mod git;
pub mod locks;
pub mod logging;
pub mod paths;
pub mod validation;

/// Format bytes as human-readable size.
pub fn human_size(bytes: u64) -> String {
    match bytes {
        0 => "0 B".into(),
        1..=1023 => format!("{} B", bytes),
        1024..=1048575 => format!("{:.1} KB", (bytes as f64) / 1024.0),
        _ => format!("{:.1} MB", (bytes as f64) / (1024.0 * 1024.0)),
    }
}
