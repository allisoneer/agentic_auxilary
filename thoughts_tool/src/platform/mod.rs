mod constants;
mod detector;

pub use constants::*;
pub use detector::{Platform, PlatformInfo, detect_platform};

// Export platform-specific info types (needed by mount managers)
#[cfg(all(target_os = "macos", not(test)))]
pub use detector::MacOSInfo;

// Export both for tests on all platforms
#[cfg(test)]
pub use detector::LinuxInfo;
