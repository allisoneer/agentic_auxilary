use crate::utils::paths::ensure_dir;
use anyhow::Result;
use std::io::ErrorKind;
use std::path::Path;

pub fn ensure_mount_dir(path: &Path) -> Result<()> {
    ensure_dir(path).map_err(|error| add_mount_repair_context(path, error))
}

fn add_mount_repair_context(path: &Path, error: anyhow::Error) -> anyhow::Error {
    if is_likely_inaccessible_mount_error(&error) {
        error.context(format!(
            "Mount directory {} exists but is not accessible.\n\
This may be a stale/disconnected FUSE mount (for example mergerfs).\n\
Repair or unmount/remount the stale mount, then run:\n\
  thoughts mount update\n\
or:\n\
  thoughts sync\n\
If this repository is already initialized, you likely do not need to run `thoughts init` again.",
            path.display()
        ))
    } else {
        error
    }
}

fn is_likely_inaccessible_mount_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(is_likely_inaccessible_mount_io_error)
    })
}

fn is_likely_inaccessible_mount_io_error(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::AlreadyExists) || is_disconnected_mount_raw_error(error)
}

fn is_disconnected_mount_raw_error(error: &std::io::Error) -> bool {
    error
        .raw_os_error()
        .is_some_and(|raw| DISCONNECTED_MOUNT_RAW_ERRORS.contains(&raw))
}

#[cfg(target_os = "linux")]
// Linux errno values: ENOTCONN = 107, ESTALE = 116.
const DISCONNECTED_MOUNT_RAW_ERRORS: &[i32] = &[107, 116];

#[cfg(target_os = "macos")]
// Darwin errno values: ENOTCONN = 57, ESTALE = 70.
const DISCONNECTED_MOUNT_RAW_ERRORS: &[i32] = &[57, 70];

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
// No disconnected-mount raw errno mappings are defined on unsupported targets.
const DISCONNECTED_MOUNT_RAW_ERRORS: &[i32] = &[];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_mount_repair_context_is_actionable_for_existing_inaccessible_dir() {
        let path = Path::new(".thoughts-data/thoughts");
        let error = anyhow::anyhow!(std::io::Error::from(ErrorKind::AlreadyExists));
        let error = add_mount_repair_context(path, error).to_string();

        assert!(error.contains(".thoughts-data/thoughts"));
        assert!(error.contains("stale/disconnected FUSE mount"));
        assert!(error.contains("thoughts mount update"));
        assert!(error.contains("thoughts sync"));
        assert!(error.contains("do not need to run `thoughts init` again"));
    }

    #[test]
    fn test_add_mount_repair_context_is_actionable_for_disconnected_mount_raw_error() {
        let Some(raw) = DISCONNECTED_MOUNT_RAW_ERRORS.first() else {
            return;
        };
        let path = Path::new(".thoughts-data/thoughts");
        let error = anyhow::anyhow!(std::io::Error::from_raw_os_error(*raw));
        let error = add_mount_repair_context(path, error).to_string();

        assert!(error.contains(".thoughts-data/thoughts"));
        assert!(error.contains("stale/disconnected FUSE mount"));
        assert!(error.contains("thoughts mount update"));
    }

    #[test]
    fn test_add_mount_repair_context_leaves_unrelated_errors_unchanged() {
        let path = Path::new(".thoughts-data/thoughts");
        let error = anyhow::anyhow!("Path exists but is not a directory: {}", path.display());
        let error = add_mount_repair_context(path, error).to_string();

        assert_eq!(
            error,
            "Path exists but is not a directory: .thoughts-data/thoughts"
        );
    }
}
