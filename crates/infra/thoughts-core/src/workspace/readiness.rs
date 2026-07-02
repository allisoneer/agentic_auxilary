use crate::config::ContextMount;
use crate::config::MountDirsV2;
use crate::config::ReferenceMount;
use crate::config::RepoConfigManager;
use crate::config::ThoughtsMount;
use crate::config::extract_org_repo_from_url;
use crate::git::utils::get_control_repo_root;
use crate::git::utils::get_current_repo;
use crate::git::utils::is_worktree;
use crate::mount::MountInfo;
use crate::mount::MountSpace;
use crate::mount::MountStatus;
use crate::mount::auto_mount::update_active_mounts;
use crate::mount::ensure_mount_dir;
use crate::mount::get_mount_manager;
use crate::platform::detect_platform;
use anyhow::Context;
use anyhow::Result;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

pub async fn ensure_thoughts_environment_ready() -> Result<()> {
    ensure_thoughts_environment_ready_inner()
        .await
        .map_err(|error| wrap_readiness_failure(&error))
}

async fn ensure_thoughts_environment_ready_inner() -> Result<()> {
    let repo_root = get_current_repo().context("Not in a git repository. Run 'git init' first.")?;
    let control_root = get_control_repo_root(&repo_root)?;
    let repo_config_manager = RepoConfigManager::new(control_root.clone());
    let desired = repo_config_manager.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;
    let thoughts_mount = desired.thoughts_mount.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "No thoughts_mount configured in repository configuration.\n\
             Add thoughts_mount to .thoughts/config.json and run 'thoughts mount update'."
        )
    })?;

    ensure_safe_repo_layout(
        &repo_root,
        &control_root,
        &desired.mount_dirs,
        thoughts_mount,
    )?;

    update_active_mounts().await?;

    verify_expected_mounts_are_ready(
        &control_root,
        &desired.mount_dirs,
        thoughts_mount,
        &desired.context_mounts,
        &desired.references,
    )
    .await
}

fn wrap_readiness_failure(error: &anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "Thoughts environment readiness check failed. Stop and repair the environment before retrying any thoughts MCP tool.\n\
         Recommended steps:\n\
           1. Run 'thoughts init'.\n\
           2. Run 'thoughts mount update' (or 'thoughts sync').\n\
           3. If symlinks point to the wrong place or collide with files/directories, repair them manually or re-run 'thoughts init --force' where appropriate.\n\
         Details:\n{error:#}"
    )
}

fn ensure_safe_repo_layout(
    repo_root: &Path,
    control_root: &Path,
    mount_dirs: &MountDirsV2,
    _thoughts_mount: &ThoughtsMount,
) -> Result<()> {
    validate_mount_dirs(mount_dirs)?;
    let is_worktree = is_worktree(repo_root)?;
    let control_data_root = control_root.join(".thoughts-data");
    ensure_directory_path(&control_data_root)?;

    for target_dir in [
        control_data_root.join(&mount_dirs.thoughts),
        control_data_root.join(&mount_dirs.context),
        control_data_root.join(&mount_dirs.references),
    ] {
        ensure_directory_path(&target_dir)?;
    }

    if is_worktree {
        ensure_symlink_target(
            &repo_root.join(".thoughts-data"),
            &control_data_root,
            &SymlinkTarget::Absolute(control_data_root.clone()),
        )?;
    } else {
        ensure_directory_path(&repo_root.join(".thoughts-data"))?;
    }

    ensure_workspace_symlink(
        &repo_root.join(&mount_dirs.thoughts),
        &control_data_root.join(&mount_dirs.thoughts),
        &format!(".thoughts-data/{}", mount_dirs.thoughts),
    )?;
    ensure_workspace_symlink(
        &repo_root.join(&mount_dirs.context),
        &control_data_root.join(&mount_dirs.context),
        &format!(".thoughts-data/{}", mount_dirs.context),
    )?;
    ensure_workspace_symlink(
        &repo_root.join(&mount_dirs.references),
        &control_data_root.join(&mount_dirs.references),
        &format!(".thoughts-data/{}", mount_dirs.references),
    )?;
    Ok(())
}

fn ensure_directory_path(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure_mount_dir(path)?;
            return Ok(());
        }
        Err(error) => {
            return Err(anyhow::Error::from(error))
                .with_context(|| format!("Failed to inspect {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "Path collision at {}: expected a directory but found a symlink.",
            path.display()
        );
    }
    if !metadata.is_dir() {
        anyhow::bail!(
            "Path collision at {}: expected a directory but found a non-directory entry.",
            path.display()
        );
    }

    ensure_mount_dir(path)?;
    Ok(())
}

fn ensure_workspace_symlink(
    link: &Path,
    absolute_target: &Path,
    relative_target: &str,
) -> Result<()> {
    ensure_symlink_target(
        link,
        absolute_target,
        &SymlinkTarget::Relative(relative_target.to_string()),
    )
}

fn ensure_symlink_target(
    link: &Path,
    absolute_target: &Path,
    create_target: &SymlinkTarget,
) -> Result<()> {
    let metadata = match fs::symlink_metadata(link) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = link.parent() {
                ensure_mount_dir(parent)?;
            }
            create_symlink(create_target, link)?;
            return Ok(());
        }
        Err(error) => {
            return Err(anyhow::Error::from(error))
                .with_context(|| format!("Failed to inspect {}", link.display()));
        }
    };
    if !metadata.file_type().is_symlink() {
        anyhow::bail!(
            "Path collision at {}: expected a symlink but found a non-symlink entry.",
            link.display()
        );
    }

    let current_target = fs::read_link(link)
        .with_context(|| format!("Failed to read symlink target for {}", link.display()))?;
    let resolved_link = fs::canonicalize(link);
    let resolved_expected = fs::canonicalize(absolute_target);
    let points_to_expected = match (resolved_link, resolved_expected) {
        (Ok(link_path), Ok(expected_path)) => link_path == expected_path,
        _ => match &create_target {
            SymlinkTarget::Absolute(path) => current_target == *path,
            SymlinkTarget::Relative(path) => current_target == Path::new(path),
        },
    };

    if !points_to_expected {
        anyhow::bail!(
            "Unsafe symlink state at {}: points to {} but must point to {}.",
            link.display(),
            current_target.display(),
            display_target(create_target)
        );
    }

    Ok(())
}

#[derive(Debug, Clone)]
enum SymlinkTarget {
    Absolute(PathBuf),
    Relative(String),
}

fn display_target(target: &SymlinkTarget) -> String {
    match target {
        SymlinkTarget::Absolute(path) => path.display().to_string(),
        SymlinkTarget::Relative(path) => path.clone(),
    }
}

fn create_symlink(target: &SymlinkTarget, link: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        match target {
            SymlinkTarget::Absolute(path) => std::os::unix::fs::symlink(path, link),
            SymlinkTarget::Relative(path) => std::os::unix::fs::symlink(path, link),
        }
        .with_context(|| {
            format!(
                "Failed to create symlink {} -> {}",
                link.display(),
                display_target(target)
            )
        })?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = target;
        let _ = link;
        anyhow::bail!("Thoughts readiness symlink support requires a Unix platform.")
    }
}

async fn verify_expected_mounts_are_ready(
    control_root: &Path,
    mount_dirs: &MountDirsV2,
    thoughts_mount: &ThoughtsMount,
    context_mounts: &[ContextMount],
    references: &[ReferenceMount],
) -> Result<()> {
    let platform_info = detect_platform()?;
    let mount_manager = get_mount_manager(&platform_info)?;
    let active_mounts = mount_manager.list_mounts().await?;

    verify_expected_mounts(
        control_root,
        mount_dirs,
        thoughts_mount,
        context_mounts,
        references,
        &active_mounts,
    )
}

fn verify_expected_mounts(
    control_root: &Path,
    mount_dirs: &MountDirsV2,
    thoughts_mount: &ThoughtsMount,
    context_mounts: &[ContextMount],
    references: &[ReferenceMount],
    active_mounts: &[MountInfo],
) -> Result<()> {
    validate_mount_dirs(mount_dirs)?;
    let expected_mounts =
        expected_mount_spaces(mount_dirs, thoughts_mount, context_mounts, references)?;
    let control_data_root = control_root.join(".thoughts-data");
    let control_data_root_canon =
        fs::canonicalize(&control_data_root).unwrap_or_else(|_| control_data_root.clone());
    let mut active_by_key = HashMap::new();

    for mount in active_mounts {
        let target_canon = fs::canonicalize(&mount.target).unwrap_or_else(|_| mount.target.clone());
        if target_canon.starts_with(&control_data_root_canon)
            && let Ok(relative) = target_canon.strip_prefix(&control_data_root_canon)
        {
            let key = relative
                .to_string_lossy()
                .trim_start_matches('/')
                .to_string();
            active_by_key.insert(key, mount);
        }
    }

    for expected in expected_mounts {
        let key = expected.relative_path(mount_dirs);
        let target = control_root.join(".thoughts-data").join(&key);
        ensure_mount_dir(&target)?;
        let active = active_by_key.get(&key).ok_or_else(|| {
            anyhow::anyhow!(
                "Required mount '{}' is not active at {}. Run 'thoughts mount update' or 'thoughts sync'.",
                expected,
                target.display()
            )
        })?;

        if active.status != MountStatus::Mounted {
            anyhow::bail!(
                "Required mount '{}' is not healthy at {} (status: {:?}). Run 'thoughts mount update' or 'thoughts sync'.",
                expected,
                target.display(),
                active.status
            );
        }
    }

    Ok(())
}

fn expected_mount_spaces(
    mount_dirs: &MountDirsV2,
    _thoughts_mount: &ThoughtsMount,
    context_mounts: &[ContextMount],
    references: &[ReferenceMount],
) -> Result<Vec<MountSpace>> {
    validate_mount_dirs(mount_dirs)?;
    let mut spaces = vec![MountSpace::Thoughts];
    for mount in context_mounts {
        spaces.push(MountSpace::Context(mount.mount_path.clone()));
    }
    for reference in references {
        let (org_path, repo) = extract_org_repo_from_url(&reference.remote).with_context(|| {
            format!(
                "Invalid reference configured for readiness checks: {}",
                reference.remote
            )
        })?;
        let ref_key = reference
            .ref_name
            .as_deref()
            .map(crate::git::ref_key::encode_ref_key)
            .transpose()?;
        spaces.push(MountSpace::Reference {
            org_path,
            repo,
            ref_key,
        });
    }

    Ok(spaces)
}

fn validate_mount_dirs(mount_dirs: &MountDirsV2) -> Result<()> {
    for (name, value) in [
        ("thoughts", mount_dirs.thoughts.as_str()),
        ("context", mount_dirs.context.as_str()),
        ("references", mount_dirs.references.as_str()),
    ] {
        validate_mount_dir_name(name, value)?;
    }

    if mount_dirs
        .thoughts
        .eq_ignore_ascii_case(&mount_dirs.context)
        || mount_dirs
            .thoughts
            .eq_ignore_ascii_case(&mount_dirs.references)
        || mount_dirs
            .context
            .eq_ignore_ascii_case(&mount_dirs.references)
    {
        anyhow::bail!("Mount directories must be distinct (thoughts/context/references)");
    }

    Ok(())
}

fn validate_mount_dir_name(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("Mount directory '{name}' cannot be empty");
    }
    if !value.is_ascii() {
        anyhow::bail!("Mount directory '{name}' must contain only ASCII characters");
    }
    if value.eq_ignore_ascii_case(".thoughts-data") {
        anyhow::bail!("Mount directory '{name}' cannot be named '.thoughts-data'");
    }
    if value.contains('/') || value.contains('\\') {
        anyhow::bail!("Mount directory '{name}' must be a single path segment (got {value})");
    }

    let mut components = Path::new(value).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(segment)), None) if segment == OsStr::new(value) => Ok(()),
        _ => anyhow::bail!(
            "Mount directory '{name}' must be a single safe path segment (got {value})"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SyncStrategy;
    use crate::mount::MountMetadata;
    use tempfile::TempDir;

    fn sample_thoughts_mount() -> ThoughtsMount {
        ThoughtsMount {
            remote: "https://github.com/example/thoughts.git".to_string(),
            subpath: None,
            sync: SyncStrategy::None,
        }
    }

    fn mounted_info(target: PathBuf) -> MountInfo {
        MountInfo {
            target,
            sources: vec![],
            status: MountStatus::Mounted,
            fs_type: "fuse.mergerfs".to_string(),
            options: vec![],
            mounted_at: None,
            pid: None,
            metadata: MountMetadata::Unknown,
        }
    }

    #[test]
    fn creates_missing_symlinks_when_absent() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &MountDirsV2::default(),
            &sample_thoughts_mount(),
        )
        .unwrap();

        assert!(repo_root.join(".thoughts-data").is_dir());
        assert_eq!(
            fs::read_link(repo_root.join("thoughts")).unwrap(),
            PathBuf::from(".thoughts-data/thoughts")
        );
        assert_eq!(
            fs::read_link(repo_root.join("context")).unwrap(),
            PathBuf::from(".thoughts-data/context")
        );
        assert_eq!(
            fs::read_link(repo_root.join("references")).unwrap(),
            PathBuf::from(".thoughts-data/references")
        );
    }

    #[test]
    fn fails_on_wrong_target_symlink() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();
        ensure_directory_path(&repo_root.join(".thoughts-data")).unwrap();
        std::os::unix::fs::symlink("somewhere-else", repo_root.join("thoughts")).unwrap();

        let error = ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &MountDirsV2::default(),
            &sample_thoughts_mount(),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("Unsafe symlink state"));
        assert!(error.contains("thoughts"));
    }

    #[test]
    fn fails_on_path_collision() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();
        fs::write(repo_root.join("context"), "collision").unwrap();

        let error = ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &MountDirsV2::default(),
            &sample_thoughts_mount(),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("Path collision"));
        assert!(error.contains("context"));
    }

    #[test]
    fn rejects_absolute_mount_dir_names() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let mount_dirs = MountDirsV2 {
            thoughts: "/tmp/escape".to_string(),
            ..MountDirsV2::default()
        };

        let error = ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &mount_dirs,
            &sample_thoughts_mount(),
        )
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("single path segment") || error.contains("single safe path segment")
        );
    }

    #[test]
    fn rejects_parent_traversal_mount_dir_names() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let mount_dirs = MountDirsV2 {
            context: "..".to_string(),
            ..MountDirsV2::default()
        };

        let error = ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &mount_dirs,
            &sample_thoughts_mount(),
        )
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("single safe path segment") || error.contains("cannot be '.' or '..'")
        );
    }

    #[test]
    fn rejects_multi_segment_mount_dir_names() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let mount_dirs = MountDirsV2 {
            references: "nested/path".to_string(),
            ..MountDirsV2::default()
        };

        let error = ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &mount_dirs,
            &sample_thoughts_mount(),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("single path segment"));
    }

    #[test]
    fn rejects_case_insensitive_reserved_mount_dir_names() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let mount_dirs = MountDirsV2 {
            thoughts: ".Thoughts-data".to_string(),
            ..MountDirsV2::default()
        };

        let error = ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &mount_dirs,
            &sample_thoughts_mount(),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("cannot be named '.thoughts-data'"));
    }

    #[test]
    fn rejects_case_insensitive_duplicate_mount_dir_names() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let mount_dirs = MountDirsV2 {
            thoughts: "thoughts".to_string(),
            context: "Thoughts".to_string(),
            references: "references".to_string(),
        };

        let error = ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &mount_dirs,
            &sample_thoughts_mount(),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("must be distinct"));
    }

    #[test]
    fn rejects_non_ascii_mount_dir_names() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();

        let mount_dirs = MountDirsV2 {
            references: "références".to_string(),
            ..MountDirsV2::default()
        };

        let error = ensure_safe_repo_layout(
            &repo_root,
            &repo_root,
            &mount_dirs,
            &sample_thoughts_mount(),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("only ASCII characters"));
    }

    #[test]
    fn verify_expected_mounts_requires_active_mounts_after_reconcile() {
        let temp = TempDir::new().unwrap();
        let control_root = temp.path().join("repo");
        fs::create_dir_all(&control_root).unwrap();
        ensure_safe_repo_layout(
            &control_root,
            &control_root,
            &MountDirsV2::default(),
            &sample_thoughts_mount(),
        )
        .unwrap();

        let error = verify_expected_mounts(
            &control_root,
            &MountDirsV2::default(),
            &sample_thoughts_mount(),
            &[],
            &[],
            &[],
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("Required mount 'thoughts' is not active"));
    }

    #[test]
    fn verify_expected_mounts_accepts_present_active_mounts() {
        let temp = TempDir::new().unwrap();
        let control_root = temp.path().join("repo");
        fs::create_dir_all(&control_root).unwrap();
        ensure_safe_repo_layout(
            &control_root,
            &control_root,
            &MountDirsV2::default(),
            &sample_thoughts_mount(),
        )
        .unwrap();
        let thoughts_target = control_root.join(".thoughts-data/thoughts");

        verify_expected_mounts(
            &control_root,
            &MountDirsV2::default(),
            &sample_thoughts_mount(),
            &[],
            &[],
            &[mounted_info(thoughts_target)],
        )
        .unwrap();
    }
}
