//! Recipe execution logic.
//!
//! Handles recipe resolution, ambiguity detection, argument mapping, and execution.

use super::cache::JustRegistry;
use super::parser::ParamKind;
use super::security::SecurityValidator;
use super::types::ExecuteOutput;
use crate::paths;
use serde_json::Value;
use std::collections::HashMap;
use tokio::process::Command;

/// Execute a recipe by name, with optional directory disambiguation and arguments.
///
/// When no dir is specified, defaults to the root repository's justfile if the recipe exists there.
/// Only errors for ambiguity if recipe is not in root AND exists in multiple subdirectories.
pub async fn execute_recipe(
    registry: &JustRegistry,
    recipe_name: &str,
    dir_opt: Option<String>,
    args_opt: Option<HashMap<String, Value>>,
    repo_root: &str,
) -> Result<ExecuteOutput, String> {
    // Canonicalize to resolve symlinks (e.g., /var -> /private/var on macOS)
    let repo_root = paths::to_abs_string(repo_root)?;
    let all = registry.get_all_recipes(&repo_root).await?;

    // Filter by name and visibility
    let mut candidates: Vec<_> = all
        .into_iter()
        .filter(|(_, r)| r.name == recipe_name && !r.is_private && !r.is_mcp_hidden)
        .collect();

    // Apply dir filter if provided
    if let Some(ref dir) = dir_opt {
        let abs_dir = paths::to_abs_string(dir)?;
        candidates.retain(|(d, _)| d == &abs_dir);
    }

    if candidates.is_empty() {
        return Err(format!(
            "Recipe '{}' not found or not exposed. Use just_search(query='{}') to discover available recipes.",
            recipe_name, recipe_name
        ));
    }

    // Select recipe: prefer root dir if no dir specified and multiple exist
    let unique_dirs: std::collections::HashSet<_> =
        candidates.iter().map(|(d, _)| d.as_str()).collect();

    let chosen_idx = if unique_dirs.len() > 1 && dir_opt.is_none() {
        // No dir specified and multiple candidates - prefer root repo justfile
        if let Some(idx) = candidates.iter().position(|(d, _)| d == &repo_root) {
            idx
        } else {
            // Recipe not in root, genuine ambiguity
            let dirs_list = unique_dirs
                .into_iter()
                .map(|d| format!("  - {}", d))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!(
                "Recipe '{}' not in root justfile and exists in multiple directories:\n{}\nSpecify dir parameter to disambiguate.",
                recipe_name, dirs_list
            ));
        }
    } else {
        0
    };

    let (chosen_dir, recipe) = candidates.swap_remove(chosen_idx);

    // Validate args
    let args = args_opt.unwrap_or_default();
    SecurityValidator::default().validate(&args)?;

    // Build argv
    let mut argv = vec![recipe_name.to_string()];
    for p in &recipe.params {
        if let Some(val) = args.get(&p.name) {
            match p.kind {
                ParamKind::Star => {
                    if let Value::Array(items) = val {
                        for item in items {
                            argv.push(value_to_arg(item)?);
                        }
                    } else {
                        argv.push(value_to_arg(val)?);
                    }
                }
                ParamKind::Singular => {
                    argv.push(value_to_arg(val)?);
                }
            }
        } else if !p.has_default {
            return Err(format!(
                "Missing required argument '{}' for recipe '{}'",
                p.name, recipe_name
            ));
        }
    }

    let output = Command::new("just")
        .args(&argv)
        .current_dir(&chosen_dir)
        .output()
        .await
        .map_err(|e| format!("Failed to execute just: {e}"))?;

    Ok(ExecuteOutput {
        dir: chosen_dir,
        recipe: recipe_name.to_string(),
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn value_to_arg(v: &Value) -> Result<String, String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err(format!("Unsupported argument type: {}", v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn value_to_arg_string() {
        assert_eq!(value_to_arg(&json!("hello")).unwrap(), "hello");
    }

    #[test]
    fn value_to_arg_number() {
        assert_eq!(value_to_arg(&json!(42)).unwrap(), "42");
        assert_eq!(value_to_arg(&json!(3.5)).unwrap(), "3.5");
    }

    #[test]
    fn value_to_arg_bool() {
        assert_eq!(value_to_arg(&json!(true)).unwrap(), "true");
        assert_eq!(value_to_arg(&json!(false)).unwrap(), "false");
    }

    #[test]
    fn value_to_arg_rejects_complex() {
        assert!(value_to_arg(&json!({"key": "value"})).is_err());
        assert!(value_to_arg(&json!(["a", "b"])).is_err());
        assert!(value_to_arg(&json!(null)).is_err());
    }

    #[tokio::test]
    async fn recipe_not_found_error() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("justfile"), "build:\n    echo building").unwrap();

        let registry = JustRegistry::new();
        let result =
            execute_recipe(&registry, "nonexistent", None, None, root.to_str().unwrap()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not found"));
        assert!(err.contains("nonexistent"));
    }

    #[tokio::test]
    async fn defaults_to_root_when_recipe_in_multiple_dirs() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create same recipe in root and subdirectory
        fs::write(root.join("justfile"), "check:\n    echo root").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/justfile"), "check:\n    echo sub").unwrap();

        let registry = JustRegistry::new();
        // No dir specified - should default to root
        let result = execute_recipe(&registry, "check", None, None, root.to_str().unwrap()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
        assert!(output.stdout.contains("root"));
    }

    #[tokio::test]
    async fn ambiguous_recipe_not_in_root_errors() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create recipe only in subdirectories, NOT in root
        fs::write(root.join("justfile"), "other:\n    echo other").unwrap();
        fs::create_dir(root.join("sub1")).unwrap();
        fs::write(root.join("sub1/justfile"), "check:\n    echo sub1").unwrap();
        fs::create_dir(root.join("sub2")).unwrap();
        fs::write(root.join("sub2/justfile"), "check:\n    echo sub2").unwrap();

        let registry = JustRegistry::new();
        // No dir specified, recipe not in root - should error
        let result = execute_recipe(&registry, "check", None, None, root.to_str().unwrap()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not in root"));
        assert!(err.contains("multiple directories"));
    }

    #[tokio::test]
    async fn missing_required_arg_error() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Recipe with required parameter
        fs::write(
            root.join("justfile"),
            "greet name:\n    echo hello {{name}}",
        )
        .unwrap();

        let registry = JustRegistry::new();
        // Call without providing the required arg
        let result = execute_recipe(&registry, "greet", None, None, root.to_str().unwrap()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Missing required argument"));
        assert!(err.contains("name"));
    }

    #[tokio::test]
    async fn successful_execution() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("justfile"), "hello:\n    echo hello world").unwrap();

        let registry = JustRegistry::new();
        let result = execute_recipe(&registry, "hello", None, None, root.to_str().unwrap()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
        assert_eq!(output.exit_code, Some(0));
        assert!(output.stdout.contains("hello world"));
    }

    #[tokio::test]
    async fn non_zero_exit_code() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("justfile"), "fail:\n    exit 42").unwrap();

        let registry = JustRegistry::new();
        let result = execute_recipe(&registry, "fail", None, None, root.to_str().unwrap()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.success);
        // just wraps the exit code, so we check it's non-zero
        assert!(output.exit_code.map(|c| c != 0).unwrap_or(true));
    }

    #[tokio::test]
    async fn disambiguate_with_dir() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create same recipe in two directories
        fs::write(root.join("justfile"), "check:\n    echo root").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/justfile"), "check:\n    echo sub").unwrap();

        let registry = JustRegistry::new();

        // Specify dir to disambiguate
        let sub_dir = root.join("sub").to_string_lossy().to_string();
        let result = execute_recipe(
            &registry,
            "check",
            Some(sub_dir),
            None,
            root.to_str().unwrap(),
        )
        .await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
        assert!(output.stdout.contains("sub"));
    }
}
