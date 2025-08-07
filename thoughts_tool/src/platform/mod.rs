mod constants;
mod detector;

pub use constants::*;
pub use detector::{LinuxInfo, Platform, PlatformInfo, detect_platform};
