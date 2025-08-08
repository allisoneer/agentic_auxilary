pub mod auto_mount;
mod manager;
pub mod resolver;
mod types;
mod utils;

#[cfg(target_os = "linux")]
mod mergerfs;

#[cfg(target_os = "macos")]
mod fuse_t;

#[cfg(test)]
mod mock;

pub use manager::{MountManager, get_mount_manager};
pub use resolver::MountResolver;
pub use types::*;
// pub use utils::*;

// Re-export implementations for direct use if needed
// #[cfg(target_os = "linux")]
// pub use mergerfs::MergerfsManager;

#[cfg(target_os = "macos")]
pub use fuse_t::FuseTManager;

#[cfg(test)]
pub use mock::MockMountManager;
