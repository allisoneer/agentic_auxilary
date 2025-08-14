mod constants;
mod detector;

pub use constants::*;
pub use detector::{Platform, PlatformInfo, detect_platform};

// Export these only for tests
#[cfg(test)]
pub use detector::{LinuxInfo, MacOSInfo};
