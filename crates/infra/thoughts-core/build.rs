use rustc_version::{Channel, version_meta};

fn main() {
    // Set platform-specific compilation flags
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-cfg=feature=\"linux\"");
    } else if cfg!(target_os = "macos") {
        println!("cargo:rustc-cfg=feature=\"macos\"");
    }

    // Ensure we're using a recent enough Rust version
    let version = version_meta().unwrap();
    if version.channel == Channel::Stable {
        assert!(version.semver.major >= 1);
        assert!(version.semver.minor >= 70);
    }

    println!("cargo:rerun-if-changed=build.rs");
}
