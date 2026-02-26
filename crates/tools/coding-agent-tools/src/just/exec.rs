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
#[expect(
    clippy::implicit_hasher,
    reason = "HashMap with default hasher is simpler for MCP tool API"
)]
#[expect(
    clippy::similar_names,
    reason = "args and argv are distinct: args is input, argv is CLI"
)]
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
            "Recipe '{recipe_name}' not found or not exposed. Use just_search(query='{recipe_name}') to discover available recipes."
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
                .map(|d| format!("  - {d}"))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!(
                "Recipe '{recipe_name}' not in root justfile and exists in multiple directories:\n{dirs_list}\nSpecify dir parameter to disambiguate."
            ));
        }
    } else {
        0
    };

    let (chosen_dir, recipe) = candidates.swap_remove(chosen_idx);

    // Validate args
    let args = args_opt.unwrap_or_default();
    SecurityValidator::default().validate(&args)?;

    // Compute expected param names and find unused arg keys for better error messages
    let expected_params: std::collections::HashSet<&str> =
        recipe.params.iter().map(|p| p.name.as_str()).collect();
    let unused_keys: Vec<&str> = args
        .keys()
        .filter(|k| !expected_params.contains(k.as_str()))
        .map(std::string::String::as_str)
        .collect();

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
            use std::fmt::Write;
            let mut err_msg = format!(
                "Missing required argument '{}' for recipe '{}'.",
                p.name, recipe_name
            );
            if !unused_keys.is_empty() {
                let _ = write!(
                    err_msg,
                    " You provided key(s) {unused_keys:?} which didn't match any parameter."
                );
            }
            let param_names: Vec<&str> = recipe.params.iter().map(|p| p.name.as_str()).collect();
            let _ = write!(err_msg, " Expected parameter(s): {param_names:?}");
            return Err(err_msg);
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
        Value::Array(_) => Err("Arrays are only supported for variadic (*) parameters".to_string()),
        Value::Null => Err("Null values are not supported; use empty string instead".to_string()),
        Value::Object(_) => Err("Object arguments are not supported".to_string()),
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    /// Skip test if `just` command is not available
    macro_rules! skip_if_just_unavailable {
        () => {
            if tokio::process::Command::new("just")
                .arg("--version")
                .output()
                .await
                .is_err()
            {
                eprintln!("Skipping test: just not installed");
                return;
            }
        };
    }

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
    fn value_to_arg_rejects_complex_with_clear_messages() {
        let obj_err = value_to_arg(&json!({"key": "value"})).unwrap_err();
        assert!(obj_err.contains("Object"));

        let arr_err = value_to_arg(&json!(["a", "b"])).unwrap_err();
        assert!(arr_err.contains("variadic"));

        let null_err = value_to_arg(&json!(null)).unwrap_err();
        assert!(null_err.contains("empty string"));
    }

    #[tokio::test]
    async fn recipe_not_found_error() {
        skip_if_just_unavailable!();

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
        skip_if_just_unavailable!();

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create same recipe in root and subdirectory
        fs::write(root.join("justfile"), "check:\n    echo root").unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
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
        skip_if_just_unavailable!();

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create recipe only in subdirectories, NOT in root
        fs::write(root.join("justfile"), "other:\n    echo other").unwrap();
        fs::create_dir_all(root.join("sub1")).unwrap();
        fs::write(root.join("sub1/justfile"), "check:\n    echo sub1").unwrap();
        fs::create_dir_all(root.join("sub2")).unwrap();
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
        skip_if_just_unavailable!();

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
        skip_if_just_unavailable!();

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
        skip_if_just_unavailable!();

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("justfile"), "fail:\n    exit 42").unwrap();

        let registry = JustRegistry::new();
        let result = execute_recipe(&registry, "fail", None, None, root.to_str().unwrap()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.success);
        // just wraps the exit code, so we check it's non-zero
        assert!(output.exit_code != Some(0));
    }

    #[tokio::test]
    async fn disambiguate_with_dir() {
        skip_if_just_unavailable!();

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create same recipe in two directories
        fs::write(root.join("justfile"), "check:\n    echo root").unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
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

    #[tokio::test]
    async fn missing_arg_error_shows_unused_keys() {
        skip_if_just_unavailable!();

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Recipe with required parameter named 'target'
        fs::write(
            root.join("justfile"),
            "build target:\n    echo building {{target}}",
        )
        .unwrap();

        let registry = JustRegistry::new();
        // Call with wrong key name 'tgt' instead of 'target'
        let mut wrong_args = HashMap::new();
        wrong_args.insert("tgt".to_string(), json!("x86_64"));

        let result = execute_recipe(
            &registry,
            "build",
            None,
            Some(wrong_args),
            root.to_str().unwrap(),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should mention the missing required arg
        assert!(err.contains("Missing required argument"));
        assert!(err.contains("target"));
        // Should mention the unused key the user provided
        assert!(err.contains("tgt"));
        // Should list expected parameters
        assert!(err.contains("Expected parameter"));
    }
}
