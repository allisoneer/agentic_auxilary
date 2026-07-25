use attention_turso::Config;
use attention_turso::PathError;
use std::error::Error as StdError;
use std::path::PathBuf;

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

#[test]
fn rejects_relative_traversal_overlap_and_reader_overrun() -> TestResult {
    assert!(matches!(
        Config::new("relative/database", "/tmp/attention-backups"),
        Err(PathError::NotAbsolute)
    ));
    let root = tempfile::tempdir()?;
    let traversal = root.path().join("database").join("..").join("escape");
    assert!(matches!(
        Config::new(traversal, root.path().join("backups")),
        Err(PathError::Traversal)
    ));
    assert!(matches!(
        Config::new(
            root.path().join("storage"),
            root.path().join("storage/backups")
        ),
        Err(PathError::Overlap)
    ));
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    assert!(matches!(
        config.clone().with_reader_count(0),
        Err(PathError::ReaderCount(0))
    ));
    assert!(matches!(
        config.with_reader_count(17),
        Err(PathError::ReaderCount(17))
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_symlink_components_and_unsafe_permissions() -> TestResult {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir()?;
    let real = root.path().join("real");
    fs::create_dir_all(&real)?;
    let link = root.path().join("linked");
    symlink(&real, &link)?;
    assert!(matches!(
        Config::new(link.join("database"), root.path().join("backups")),
        Err(PathError::Symlink)
    ));

    let unsafe_directory = root.path().join("unsafe");
    fs::create_dir_all(&unsafe_directory)?;
    fs::set_permissions(&unsafe_directory, fs::Permissions::from_mode(0o755))?;
    assert!(matches!(
        Config::new(&unsafe_directory, root.path().join("safe-backups")),
        Err(PathError::UnsafePermissions)
    ));
    Ok(())
}

#[test]
fn absolute_test_path_is_platform_native() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(path.is_absolute());
}
