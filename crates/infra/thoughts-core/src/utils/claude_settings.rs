use anyhow::{Context, Result, anyhow};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use colored::Colorize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct InjectionSummary {
    pub settings_path: PathBuf,
    pub added_additional_dirs: Vec<PathBuf>,
    pub added_allow_rules: Vec<String>,
    pub already_present_additional_dirs: Vec<PathBuf>,
    pub already_present_allow_rules: Vec<String>,
    pub warn_conflicting_denies: Vec<String>,
}

/// Inject Claude Code permissions using the additionalDirectories mechanism and
/// narrow relative allow patterns. Works for both worktrees and regular repos.
/// - Adds canonical(repo_root/.thoughts-data) to permissions.additionalDirectories
/// - Adds three allow rules: Read(thoughts/**), Read(context/**), Read(references/**)
/// - Atomic, idempotent, and never panics on malformed JSON (quarantines instead)
pub fn inject_additional_directories(repo_root: &Path) -> Result<InjectionSummary> {
    let settings_path = get_local_settings_path(repo_root);
    ensure_parent_dir(&settings_path)?;

    // Resolve .thoughts-data canonical path; fallback to non-canonical on error
    let td = repo_root.join(".thoughts-data");
    let canonical_thoughts_data = match fs::canonicalize(&td) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{}: Failed to canonicalize {} ({}). Falling back to non-canonical path.",
                "Warning".yellow(),
                td.display(),
                e
            );
            td.clone()
        }
    };

    let ReadOutcome {
        mut value,
        had_valid_json,
    } = read_or_init_settings(&settings_path)?;

    // Ensure permissions scaffold (including additionalDirectories and allow arrays)
    ensure_permissions_scaffold(&mut value);

    // Prepare to track changes
    let mut added_additional_dirs = Vec::new();
    let mut already_present_additional_dirs = Vec::new();
    let mut added_allow_rules = Vec::new();
    let mut already_present_allow_rules = Vec::new();

    // Work with additionalDirectories and allow in a nested scope to avoid borrow conflicts
    {
        let permissions = value.get_mut("permissions").unwrap();

        // Ensure additionalDirectories array exists
        if !permissions
            .get("additionalDirectories")
            .map(|x| x.is_array())
            .unwrap_or(false)
        {
            permissions["additionalDirectories"] = json!([]);
        }

        let add_dirs = permissions["additionalDirectories"].as_array_mut().unwrap();

        // Build existing set for deduplication
        let mut existing_add_dirs: HashSet<String> = add_dirs
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        // 1) Insert canonical .thoughts-data path into additionalDirectories
        let dir_str = canonical_thoughts_data.to_string_lossy().to_string();
        if existing_add_dirs.contains(&dir_str) {
            already_present_additional_dirs.push(canonical_thoughts_data.clone());
        } else {
            add_dirs.push(Value::String(dir_str.clone()));
            existing_add_dirs.insert(dir_str);
            added_additional_dirs.push(canonical_thoughts_data.clone());
        }
    }

    // Now work with allow rules in a separate scope
    let warn_conflicting_denies = {
        let permissions = value.get_mut("permissions").unwrap();

        // Ensure allow array exists
        if !permissions
            .get("allow")
            .map(|x| x.is_array())
            .unwrap_or(false)
        {
            permissions["allow"] = json!([]);
        }

        let allow = permissions["allow"].as_array_mut().unwrap();

        // Build existing set for deduplication
        let mut existing_allow: HashSet<String> = allow
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        // 2) Insert narrow relative allow rules
        let required_rules = vec![
            "Read(thoughts/**)".to_string(),
            "Read(context/**)".to_string(),
            "Read(references/**)".to_string(),
        ];

        for r in required_rules {
            if existing_allow.contains(&r) {
                already_present_allow_rules.push(r);
            } else {
                allow.push(Value::String(r.clone()));
                existing_allow.insert(r.clone());
                added_allow_rules.push(r);
            }
        }

        // Best-effort conflict detection (exact string matches)
        collect_conflicting_denies(permissions, &existing_allow)
    };

    // Only write if something changed
    if !added_additional_dirs.is_empty() || !added_allow_rules.is_empty() {
        if had_valid_json && settings_path.exists() {
            backup_valid_to_bak(&settings_path)
                .with_context(|| format!("Failed to create backup for {:?}", settings_path))?;
        }
        let serialized = serde_json::to_string_pretty(&value)
            .context("Failed to serialize Claude settings JSON")?;

        AtomicFile::new(&settings_path, OverwriteBehavior::AllowOverwrite)
            .write(|f| f.write_all(serialized.as_bytes()))
            .with_context(|| format!("Failed to write {:?}", settings_path))?;
    }

    // Best-effort prune at end of operation to keep directory tidy
    if let Err(e) = prune_malformed_backups(&settings_path, 3) {
        eprintln!(
            "{}: Failed to prune malformed Claude backups: {}",
            "Warning".yellow(),
            e
        );
    }
    Ok(InjectionSummary {
        settings_path,
        added_additional_dirs,
        added_allow_rules,
        already_present_additional_dirs,
        already_present_allow_rules,
        warn_conflicting_denies,
    })
}

fn get_local_settings_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".claude").join("settings.local.json")
}

fn ensure_parent_dir(settings_path: &Path) -> Result<()> {
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }
    Ok(())
}

struct ReadOutcome {
    value: Value,
    had_valid_json: bool,
}

fn read_or_init_settings(settings_path: &Path) -> Result<ReadOutcome> {
    if !settings_path.exists() {
        return Ok(ReadOutcome {
            value: json!({}),
            had_valid_json: false,
        });
    }

    let raw = fs::read_to_string(settings_path)
        .with_context(|| format!("Failed to read {:?}", settings_path))?;

    match serde_json::from_str::<Value>(&raw) {
        Ok(value) => Ok(ReadOutcome {
            value,
            had_valid_json: true,
        }),
        Err(_) => {
            // Malformed JSON: quarantine and start fresh
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let malformed = settings_path.with_extension(format!("json.malformed.{}.bak", ts));
            let _ = fs::rename(settings_path, &malformed);
            eprintln!(
                "{}: Existing Claude settings were malformed. Quarantined to {}",
                "Warning".yellow(),
                malformed.display()
            );
            // Best-effort prune after quarantine
            if let Err(e) = prune_malformed_backups(settings_path, 3) {
                eprintln!(
                    "{}: Failed to prune malformed Claude backups: {}",
                    "Warning".yellow(),
                    e
                );
            }
            Ok(ReadOutcome {
                value: json!({}),
                had_valid_json: false,
            })
        }
    }
}

/// Ensure permissions, allow, deny, ask scaffolding exists without overwriting unrelated keys.
fn ensure_permissions_scaffold(root: &mut Value) {
    if !root.is_object() {
        *root = json!({});
    }
    if !root
        .get("permissions")
        .map(|x| x.is_object())
        .unwrap_or(false)
    {
        root["permissions"] = json!({});
    }
    if root["permissions"].get("deny").is_none() {
        root["permissions"]["deny"] = json!([]);
    }
    if root["permissions"].get("ask").is_none() {
        root["permissions"]["ask"] = json!([]);
    }
}

fn backup_valid_to_bak(settings_path: &Path) -> Result<()> {
    let bak = settings_path.with_extension("json.bak");
    fs::copy(settings_path, &bak)
        .with_context(|| format!("Failed to copy {:?} -> {:?}", settings_path, bak))?;
    Ok(())
}

fn collect_conflicting_denies(permissions: &Value, allow_set: &HashSet<String>) -> Vec<String> {
    let mut conflicts = Vec::new();
    if let Some(deny) = permissions.get("deny").and_then(|d| d.as_array()) {
        for d in deny {
            if let Some(ds) = d.as_str()
                && allow_set.contains(ds)
            {
                conflicts.push(ds.to_string());
            }
        }
    }
    conflicts
}

fn prune_malformed_backups(settings_path: &Path, keep: usize) -> Result<usize> {
    let dir = settings_path
        .parent()
        .ok_or_else(|| anyhow!("Missing parent dir for settings"))?;
    let prefix = "settings.local.json.malformed.";
    let suffix = ".bak";
    let mut entries: Vec<(u64, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("Failed to read {:?}", dir))? {
        let p = entry?.path();
        let Some(name_os) = p.file_name() else {
            continue;
        };
        let name = name_os.to_string_lossy();
        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }
        let ts_str = &name[prefix.len()..name.len() - suffix.len()];
        if let Ok(ts) = ts_str.parse::<u64>() {
            entries.push((ts, p));
        }
    }
    // Sort newest first
    entries.sort_by_key(|(ts, _)| *ts);
    entries.reverse();
    let mut deleted = 0usize;
    for (_, p) in entries.into_iter().skip(keep) {
        match fs::remove_file(&p) {
            Ok(_) => deleted += 1,
            Err(e) => eprintln!(
                "{}: Failed to remove old malformed backup {}: {}",
                "Warning".yellow(),
                p.display(),
                e
            ),
        }
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn creates_file_and_adds_additional_dir_and_rules() {
        let td = TempDir::new().unwrap();
        let repo = td.path();

        // Create .thoughts-data to allow canonicalization
        let td_path = repo.join(".thoughts-data");
        fs::create_dir_all(&td_path).unwrap();

        let summary = inject_additional_directories(repo).unwrap();

        // Path correctness
        assert!(
            summary
                .settings_path
                .ends_with(".claude/settings.local.json")
        );

        // Should have added both: at least one additional dir and all rules
        assert_eq!(summary.added_additional_dirs.len(), 1);
        assert_eq!(summary.added_allow_rules.len(), 3);

        let content = fs::read_to_string(&summary.settings_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        let add_dirs = json["permissions"]["additionalDirectories"]
            .as_array()
            .unwrap();
        let allow = json["permissions"]["allow"].as_array().unwrap();

        // Verify contents
        let add_dirs_strs: Vec<&str> = add_dirs.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(add_dirs_strs.len(), 1);
        assert!(add_dirs_strs[0].ends_with("/.thoughts-data"));

        let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
        assert!(allow_strs.contains(&"Read(thoughts/**)"));
        assert!(allow_strs.contains(&"Read(context/**)"));
        assert!(allow_strs.contains(&"Read(references/**)"));
    }

    #[test]
    fn idempotent_no_duplicates() {
        let td = TempDir::new().unwrap();
        let repo = td.path();
        fs::create_dir_all(repo.join(".thoughts-data")).unwrap();

        let _ = inject_additional_directories(repo).unwrap();
        let again = inject_additional_directories(repo).unwrap();

        assert!(again.added_additional_dirs.is_empty());
        assert!(again.added_allow_rules.is_empty());

        let content = fs::read_to_string(&again.settings_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        let allow = json["permissions"]["allow"].as_array().unwrap();

        let mut seen = std::collections::HashSet::new();
        for item in allow {
            if let Some(s) = item.as_str() {
                assert!(seen.insert(s.to_string()), "Duplicate found: {}", s);
            }
        }
    }

    #[test]
    fn malformed_settings_is_quarantined() {
        let td = TempDir::new().unwrap();
        let repo = td.path();
        fs::create_dir_all(repo.join(".thoughts-data")).unwrap();

        let settings = repo.join(".claude").join("settings.local.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, "not-json").unwrap();

        let summary = inject_additional_directories(repo).unwrap();
        assert!(summary.settings_path.exists());

        // Look for quarantine
        let dir = settings.parent().unwrap();
        let entries = fs::read_dir(dir).unwrap();
        let mut found_malformed = false;
        for e in entries {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy();
            if name.contains("settings.local.json.malformed.") {
                found_malformed = true;
                break;
            }
        }
        assert!(found_malformed);
    }

    #[test]
    fn backup_valid_before_write() {
        let td = TempDir::new().unwrap();
        let repo = td.path();
        fs::create_dir_all(repo.join(".thoughts-data")).unwrap();

        let settings = repo.join(".claude").join("settings.local.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            r#"{"permissions":{"allow":[],"deny":[],"ask":[]}}"#,
        )
        .unwrap();

        let _ = inject_additional_directories(repo).unwrap();
        let bak = settings.with_extension("json.bak");
        assert!(bak.exists());
    }

    #[test]
    fn fallback_to_non_canonical_on_missing_path() {
        let td = TempDir::new().unwrap();
        let repo = td.path();
        // Intentionally do NOT create .thoughts-data so canonicalize fails
        let summary = inject_additional_directories(repo).unwrap();

        let content = fs::read_to_string(&summary.settings_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        let add_dirs = json["permissions"]["additionalDirectories"]
            .as_array()
            .unwrap();
        let add_dirs_strs: Vec<&str> = add_dirs.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(add_dirs_strs.len(), 1);
        assert!(add_dirs_strs[0].ends_with("/.thoughts-data"));
    }

    #[test]
    fn prunes_to_last_three_malformed_backups() {
        let td = TempDir::new().unwrap();
        let repo = td.path();
        let settings = repo.join(".claude").join("settings.local.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();

        // Create 5 malformed backups with increasing timestamps
        for ts in [100, 200, 300, 400, 500] {
            let p = settings.with_extension(format!("json.malformed.{}.bak", ts));
            fs::write(&p, b"{}").unwrap();
        }

        let deleted = super::prune_malformed_backups(&settings, 3).unwrap();
        assert_eq!(deleted, 2);

        let kept: Vec<u64> = fs::read_dir(settings.parent().unwrap())
            .unwrap()
            .filter_map(|e| {
                let name = e.unwrap().file_name().to_string_lossy().into_owned();
                if let Some(s) = name
                    .strip_prefix("settings.local.json.malformed.")
                    .and_then(|s| s.strip_suffix(".bak"))
                {
                    s.parse::<u64>().ok()
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(kept.len(), 3);
        assert!(kept.contains(&300) && kept.contains(&400) && kept.contains(&500));
    }

    #[test]
    fn ignores_non_numeric_malformed_backups() {
        let td = TempDir::new().unwrap();
        let repo = td.path();
        let settings = repo.join(".claude").join("settings.local.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();

        // Badly named file should be ignored by prune
        let bad = settings.with_extension("json.malformed.bad.bak");
        fs::write(&bad, b"{}").unwrap();

        let deleted = super::prune_malformed_backups(&settings, 3).unwrap();
        assert_eq!(deleted, 0);
        assert!(bad.exists());
    }

    #[test]
    fn quarantine_then_prune_leaves_three() {
        let td = TempDir::new().unwrap();
        let repo = td.path();
        fs::create_dir_all(repo.join(".thoughts-data")).unwrap();

        // Corrupt settings multiple times to force quarantine
        for _ in 0..5 {
            let settings = repo.join(".claude").join("settings.local.json");
            fs::create_dir_all(settings.parent().unwrap()).unwrap();
            fs::write(&settings, "not-json").unwrap();
            let _ = inject_additional_directories(repo).unwrap();
        }

        // Count malformed backups (should be <= 3)
        let dir = repo.join(".claude");
        let count = fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .ok()
                    .and_then(|x| {
                        x.file_name()
                            .to_str()
                            .map(|s| s.contains("settings.local.json.malformed."))
                    })
                    .unwrap_or(false)
            })
            .count();
        assert!(count <= 3);
    }
}
