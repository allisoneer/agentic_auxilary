use crate::error::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info};

#[derive(Debug, Clone, PartialEq)]
pub enum Platform {
    Linux(LinuxInfo),
    MacOS(MacOSInfo),
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinuxInfo {
    pub distro: String,
    pub version: String,
    pub has_mergerfs: bool,
    pub mergerfs_version: Option<String>,
    pub fuse_available: bool,
    pub has_fusermount: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacOSInfo {
    pub version: String,
    pub has_fuse_t: bool,
    pub fuse_t_version: Option<String>,
    pub has_macfuse: bool,
    pub macfuse_version: Option<String>,
    pub has_unionfs: bool,
    pub unionfs_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PlatformInfo {
    pub platform: Platform,
    pub arch: String,
    pub can_mount: bool,
    pub missing_tools: Vec<String>,
}

impl Platform {
    #[allow(dead_code)]
    // Used in tests, could be useful for diagnostics
    pub fn can_mount(&self) -> bool {
        match self {
            Platform::Linux(info) => info.has_mergerfs && info.fuse_available,
            Platform::MacOS(info) => info.has_fuse_t || info.has_macfuse,
            Platform::Unsupported(_) => false,
        }
    }

    #[allow(dead_code)]
    // Could be used in error messages showing required tools
    pub fn mount_tool_name(&self) -> Option<&'static str> {
        match self {
            Platform::Linux(_) => Some("mergerfs"),
            Platform::MacOS(_) => Some("FUSE-T or macFUSE"),
            Platform::Unsupported(_) => None,
        }
    }
}

pub fn detect_platform() -> Result<PlatformInfo> {
    debug!("Starting platform detection");

    #[cfg(target_os = "linux")]
    {
        detect_linux()
    }

    #[cfg(target_os = "macos")]
    {
        detect_macos()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let os = std::env::consts::OS;
        Ok(PlatformInfo {
            platform: Platform::Unsupported(os.to_string()),
            arch: std::env::consts::ARCH.to_string(),
            can_mount: false,
            missing_tools: vec!["Platform not supported".to_string()],
        })
    }
}

#[cfg(target_os = "linux")]
fn detect_linux() -> Result<PlatformInfo> {
    // Detect distribution
    let (distro, version) = detect_linux_distro();
    info!("Detected Linux distribution: {} {}", distro, version);

    // Check for mergerfs
    let (has_mergerfs, mergerfs_version) = check_mergerfs();
    if has_mergerfs {
        info!("Found mergerfs version: {:?}", mergerfs_version);
    } else {
        info!("mergerfs not found");
    }

    // Check for FUSE support
    let fuse_available = check_fuse_support();
    if fuse_available {
        info!("FUSE support detected");
    } else {
        info!("FUSE support not detected");
    }

    // Check for fusermount
    let has_fusermount = which::which("fusermount")
        .or_else(|_| which::which("fusermount3"))
        .is_ok();
    if has_fusermount {
        info!("fusermount detected");
    }

    let mut missing_tools = Vec::new();
    if !has_mergerfs {
        missing_tools.push("mergerfs".to_string());
    }
    if !fuse_available {
        missing_tools.push("FUSE kernel module".to_string());
    }

    let linux_info = LinuxInfo {
        distro,
        version,
        has_mergerfs,
        mergerfs_version,
        fuse_available,
        has_fusermount,
    };

    Ok(PlatformInfo {
        platform: Platform::Linux(linux_info.clone()),
        arch: std::env::consts::ARCH.to_string(),
        can_mount: linux_info.has_mergerfs && linux_info.fuse_available,
        missing_tools,
    })
}

#[cfg(target_os = "linux")]
fn detect_linux_distro() -> (String, String) {
    // Try to read /etc/os-release (systemd standard)
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        let mut name = "Unknown".to_string();
        let mut version = "Unknown".to_string();

        for line in content.lines() {
            if let Some(value) = line.strip_prefix("NAME=") {
                name = value.trim_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("VERSION=") {
                version = value.trim_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("VERSION_ID=")
                && version == "Unknown"
            {
                version = value.trim_matches('"').to_string();
            }
        }

        return (name, version);
    }

    // Fallback to lsb_release if available
    if let Ok(output) = Command::new("lsb_release").args(["-d", "-r"]).output() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = output_str.lines().collect();
        let distro = lines
            .first()
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        let version = lines
            .get(1)
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        return (distro, version);
    }

    ("Unknown Linux".to_string(), "Unknown".to_string())
}

#[cfg(target_os = "linux")]
fn check_mergerfs() -> (bool, Option<String>) {
    match which::which("mergerfs") {
        Ok(path) => {
            debug!("Found mergerfs at: {:?}", path);
            // Try to get version
            if let Ok(output) = Command::new("mergerfs").arg("-V").output() {
                let version_str = String::from_utf8_lossy(&output.stdout);
                if let Some(version_line) = version_str.lines().next() {
                    let version = version_line
                        .split_whitespace()
                        .find(|s| s.chars().any(|c| c.is_ascii_digit()))
                        .map(|s| s.to_string());
                    return (true, version);
                }
            }
            (true, None)
        }
        Err(_) => (false, None),
    }
}

#[cfg(target_os = "linux")]
fn check_fuse_support() -> bool {
    // Check if FUSE module is loaded
    if Path::new("/sys/module/fuse").exists() {
        return true;
    }

    // Check if we can load the module (requires privileges)
    if Path::new("/dev/fuse").exists() {
        return true;
    }

    // Try to check with modinfo
    if let Ok(output) = Command::new("modinfo").arg("fuse").output() {
        return output.status.success();
    }

    false
}

#[cfg(target_os = "macos")]
fn detect_macos() -> Result<PlatformInfo> {
    // Get macOS version
    let version = get_macos_version();
    info!("Detected macOS version: {}", version);

    // Check for FUSE-T
    let (has_fuse_t, fuse_t_version) = check_fuse_t();
    if has_fuse_t {
        info!("Found FUSE-T version: {:?}", fuse_t_version);
    }

    // Check for macFUSE
    let (has_macfuse, macfuse_version) = check_macfuse();
    if has_macfuse {
        info!("Found macFUSE version: {:?}", macfuse_version);
    }

    // Check for unionfs-fuse
    let unionfs_path = which::which("unionfs-fuse")
        .or_else(|_| which::which("unionfs"))
        .ok();
    let has_unionfs = unionfs_path.is_some();
    if has_unionfs {
        info!("Found unionfs at: {:?}", unionfs_path);
    }

    let mut missing_tools = Vec::new();
    if !has_fuse_t && !has_macfuse {
        missing_tools.push("FUSE-T or macFUSE".to_string());
    }
    if !has_unionfs {
        missing_tools.push("unionfs-fuse".to_string());
    }

    let macos_info = MacOSInfo {
        version,
        has_fuse_t,
        fuse_t_version,
        has_macfuse,
        macfuse_version,
        has_unionfs,
        unionfs_path,
    };

    Ok(PlatformInfo {
        platform: Platform::MacOS(macos_info.clone()),
        arch: std::env::consts::ARCH.to_string(),
        can_mount: macos_info.has_fuse_t || macos_info.has_macfuse,
        missing_tools,
    })
}

#[cfg(target_os = "macos")]
fn get_macos_version() -> String {
    if let Ok(output) = Command::new("sw_vers").arg("-productVersion").output() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        "Unknown".to_string()
    }
}

#[cfg(target_os = "macos")]
fn check_fuse_t() -> (bool, Option<String>) {
    // FUSE-T detection: Check for the FUSE-T filesystem bundle
    let fuse_t_path = Path::new("/Library/Filesystems/fuse-t.fs");
    if fuse_t_path.exists() {
        // Try to get version from Info.plist
        let plist_path = fuse_t_path.join("Contents/Info.plist");
        if let Ok(content) = std::fs::read_to_string(&plist_path) {
            // Parse version from plist
            if let Some(version_start) = content.find("<key>CFBundleShortVersionString</key>") {
                if let Some(version_line) = content[version_start..].lines().nth(1) {
                    if let Some(version) = version_line
                        .trim()
                        .strip_prefix("<string>")
                        .and_then(|s| s.strip_suffix("</string>"))
                    {
                        debug!("Found FUSE-T version: {}", version);
                        return (true, Some(version.to_string()));
                    }
                }
            }
        }
        debug!("Found FUSE-T but could not determine version");
        return (true, None);
    }

    // Also check for go-nfsv4 binary (FUSE-T component)
    if Path::new("/usr/local/bin/go-nfsv4").exists() {
        debug!("Found go-nfsv4 binary (FUSE-T component)");
        return (true, None);
    }

    (false, None)
}

#[cfg(target_os = "macos")]
fn check_macfuse() -> (bool, Option<String>) {
    // Check for macFUSE installation
    let macfuse_path = Path::new("/Library/Filesystems/macfuse.fs");
    if macfuse_path.exists() {
        // Try to get version
        let plist_path = macfuse_path.join("Contents/Info.plist");
        if let Ok(content) = std::fs::read_to_string(plist_path) {
            // Parse version from plist (simplified)
            if let Some(version_start) = content.find("<key>CFBundleShortVersionString</key>") {
                if let Some(version_line) = content[version_start..].lines().nth(1) {
                    if let Some(version) = version_line
                        .trim()
                        .strip_prefix("<string>")
                        .and_then(|s| s.strip_suffix("</string>"))
                    {
                        return (true, Some(version.to_string()));
                    }
                }
            }
        }
        return (true, None);
    }

    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let info = detect_platform().unwrap();

        // Should detect something
        match &info.platform {
            Platform::Linux(_) => {
                assert_eq!(std::env::consts::OS, "linux");
            }
            Platform::MacOS(_) => {
                assert_eq!(std::env::consts::OS, "macos");
            }
            Platform::Unsupported(os) => {
                assert_eq!(os, std::env::consts::OS);
            }
        }

        // Architecture should be detected
        assert!(!info.arch.is_empty());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_mount_tool_name_linux() {
        let linux_platform = Platform::Linux(LinuxInfo {
            distro: "Ubuntu".to_string(),
            version: "22.04".to_string(),
            has_mergerfs: true,
            mergerfs_version: Some("2.33.5".to_string()),
            fuse_available: true,
            has_fusermount: true,
        });
        assert_eq!(linux_platform.mount_tool_name(), Some("mergerfs"));

        let unsupported = Platform::Unsupported("windows".to_string());
        assert_eq!(unsupported.mount_tool_name(), None);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_mount_tool_name_macos() {
        let macos_platform = Platform::MacOS(MacOSInfo {
            version: "13.0".to_string(),
            has_fuse_t: true,
            fuse_t_version: Some("1.0.0".to_string()),
            has_macfuse: false,
            macfuse_version: None,
            has_unionfs: true,
            unionfs_path: Some(PathBuf::from("/usr/local/bin/unionfs-fuse")),
        });
        assert_eq!(macos_platform.mount_tool_name(), Some("FUSE-T or macFUSE"));

        let unsupported = Platform::Unsupported("windows".to_string());
        assert_eq!(unsupported.mount_tool_name(), None);
    }
}
