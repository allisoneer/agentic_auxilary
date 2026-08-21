use attention_turso::AttentionDatabase;
use attention_turso::BackupManifest;
use attention_turso::Config;
use attention_turso::Error;
use attention_turso::ProbeResolution;
use std::error::Error as StdError;
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

async fn source_database() -> TestResult<(tempfile::TempDir, Config, BackupManifest)> {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config.clone()).await?;
    database.run_startup_migrations().await?;
    database
        .write_qualification_probe("backup-probe", b"fingerprint", b"value")
        .await?;
    assert!(matches!(
        database.backup("open-rejected"),
        Err(Error::Lifecycle)
    ));
    database.close().await?;
    let manifest = database.backup("complete")?;
    Ok((root, config, manifest))
}

#[tokio::test]
async fn stopped_backup_restores_probe_and_exact_ledger_to_new_empty_root() -> TestResult {
    let (_root, source, manifest) = source_database().await?;
    assert_eq!(manifest.format_version(), 2);
    assert_eq!(manifest.migration_head(), 5);
    assert_eq!(manifest.payload_version(), 1);
    assert!(!manifest.files().is_empty());
    let names: Vec<_> = manifest
        .files()
        .iter()
        .map(attention_turso::BackupEntry::path)
        .collect();
    assert_eq!(names, ["attention.db", "attention.db-wal"]);
    assert!(
        manifest
            .files()
            .iter()
            .any(|entry| entry.path() == "attention.db")
    );

    let target = tempfile::tempdir()?;
    let restore = Config::new(
        target.path().join("restored-database"),
        source.backup_root().as_path(),
    )?;
    let database = AttentionDatabase::restore(restore, "complete").await?;
    assert_eq!(database.run_startup_migrations().await?.applied(), 0);
    assert_eq!(
        database
            .resolve_qualification_probe("backup-probe", b"fingerprint")
            .await?,
        ProbeResolution::Matching(b"value".to_vec())
    );
    database.close().await?;
    Ok(())
}

#[tokio::test]
async fn every_missing_or_changed_required_file_is_rejected() -> TestResult {
    let (_root, source, manifest) = source_database().await?;
    let complete = source.backup_root().as_path().join("complete");
    for (index, entry) in manifest.files().iter().enumerate() {
        let missing_name = format!("missing-{index}");
        let missing = source.backup_root().as_path().join(&missing_name);
        copy_directory(&complete, &missing)?;
        fs::remove_file(missing.join(entry.path()))?;
        assert_restore_rejected(&source, &missing_name).await?;

        let changed_name = format!("changed-{index}");
        let changed = source.backup_root().as_path().join(&changed_name);
        copy_directory(&complete, &changed)?;
        fs::write(changed.join(entry.path()), b"tampered")?;
        assert_restore_rejected(&source, &changed_name).await?;
    }
    Ok(())
}

#[tokio::test]
async fn migration_head_and_checksum_manifest_drift_are_rejected() -> TestResult {
    let (_root, source, _manifest) = source_database().await?;
    let complete = source.backup_root().as_path().join("complete");
    for (name, field, value) in [
        (
            "wrong-payload-version",
            "payload_version",
            serde_json::json!(2),
        ),
        ("wrong-head", "migration_head", serde_json::json!(1)),
        (
            "wrong-migration-checksum",
            "migration_checksum",
            serde_json::json!([0, 1, 2]),
        ),
    ] {
        let changed = source.backup_root().as_path().join(name);
        copy_directory(&complete, &changed)?;
        let manifest_path = changed.join("manifest.json");
        let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        assert!(
            manifest.get(field).is_some(),
            "manifest field {field} is missing"
        );
        manifest[field] = value;
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
        assert_restore_rejected(&source, name).await?;
    }
    Ok(())
}

#[tokio::test]
async fn partial_manifest_traversal_unexpected_symlink_and_nonempty_targets_are_rejected()
-> TestResult {
    let (_root, source, _manifest) = source_database().await?;
    let complete = source.backup_root().as_path().join("complete");

    let partial = source.backup_root().as_path().join("partial");
    create_private_directory(&partial)?;
    fs::write(partial.join("attention.db"), b"partial")?;
    assert_restore_rejected(&source, "partial").await?;

    let traversal = source.backup_root().as_path().join("traversal");
    copy_directory(&complete, &traversal)?;
    let manifest_path = traversal.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    manifest["files"][0]["path"] = serde_json::Value::String("../escape".to_string());
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    assert_restore_rejected(&source, "traversal").await?;

    let unexpected = source.backup_root().as_path().join("unexpected");
    copy_directory(&complete, &unexpected)?;
    create_private_directory(&unexpected.join("directory"))?;
    assert_restore_rejected(&source, "unexpected").await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let linked = source.backup_root().as_path().join("linked");
        copy_directory(&complete, &linked)?;
        symlink("attention.db", linked.join("link"))?;
        assert_restore_rejected(&source, "linked").await?;
    }

    let target = tempfile::tempdir()?;
    let restore = Config::new(
        target.path().join("nonempty"),
        source.backup_root().as_path(),
    )?;
    fs::write(
        restore.database_directory().as_path().join("attacker"),
        b"occupied",
    )?;
    assert!(matches!(
        AttentionDatabase::restore(restore, "complete").await,
        Err(Error::Backup(_))
    ));
    assert!(matches!(
        AttentionDatabase::restore(
            Config::new(
                target.path().join("invalid-name"),
                source.backup_root().as_path()
            )?,
            "../escape"
        )
        .await,
        Err(Error::Backup(_))
    ));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn backup_permission_violation_is_rejected() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let (_root, source, _manifest) = source_database().await?;
    let backup = source.backup_root().as_path().join("complete");
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o755))?;
    assert_restore_rejected(&source, "complete").await?;
    Ok(())
}

async fn assert_restore_rejected(source: &Config, name: &str) -> TestResult {
    let target = tempfile::tempdir()?;
    let restore = Config::new(
        target.path().join("database"),
        source.backup_root().as_path(),
    )?;
    assert!(AttentionDatabase::restore(restore, name).await.is_err());
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> TestResult {
    create_private_directory(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> TestResult {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700).create(path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path)?;
    Ok(())
}
