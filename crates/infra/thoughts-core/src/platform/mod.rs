mod constants;
pub mod detector;

pub use constants::*;
pub use detector::{Platform, PlatformInfo, detect_platform};

// Platform-specific info types are available through detector module
// Use: crate::platform::detector::LinuxInfo or crate::platform::detector::MacOSInfo
