mod manager;
mod types;
mod utils;
pub mod resolver;
pub mod auto_mount;

#[cfg(target_os = "linux")]
mod mergerfs;

#[cfg(target_os = "macos")]
mod fuse_t;

#[cfg(test)]
mod mock;

pub use manager::{get_mount_manager, MountManager};
pub use types::*;
pub use resolver::MountResolver;
// pub use utils::*;

// Re-export implementations for direct use if needed
// #[cfg(target_os = "linux")]
// pub use mergerfs::MergerfsManager;

#[cfg(target_os = "macos")]
pub use fuse_t::FuseTManager;

#[cfg(test)]
pub use mock::MockMountManager;
