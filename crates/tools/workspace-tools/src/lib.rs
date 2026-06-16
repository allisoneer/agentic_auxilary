#[cfg(not(unix))]
compile_error!("workspace_tools only supports Unix-like platforms (Linux/macOS).");

pub mod patch;
pub mod paths;
pub mod service;
pub mod tools;

pub use tools::build_registry;
