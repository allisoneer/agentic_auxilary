//! Parse `just --dump --dump-format json` output.

use serde::Deserialize;
use std::collections::HashMap;
use tokio::process::Command;

#[derive(Debug, Deserialize)]
struct JustDump {
    recipes: HashMap<String, DumpRecipe>,
}

#[derive(Debug, Deserialize)]
struct DumpRecipe {
    #[serde(default)]
    doc: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    parameters: Vec<DumpParam>,
}

#[derive(Debug, Deserialize)]
struct DumpParam {
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    default: Option<serde_json::Value>,
}

/// A parsed recipe from a justfile.
#[derive(Debug, Clone)]
pub struct ParsedRecipe {
    /// Recipe name
    pub name: String,
    /// Documentation comment (if any)
    pub doc: Option<String>,
    /// Whether the recipe is private (`_prefix` or `[private]`)
    pub is_private: bool,
    /// Whether the recipe has `@mcp-hidden` in its docs
    pub is_mcp_hidden: bool,
    /// Recipe parameters
    pub params: Vec<ParsedParam>,
}

/// A parsed parameter from a recipe.
#[derive(Debug, Clone)]
pub struct ParsedParam {
    /// Parameter name
    pub name: String,
    /// Parameter kind (singular or variadic)
    pub kind: ParamKind,
    /// Whether the parameter has a default value
    pub has_default: bool,
}

/// Kind of recipe parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// Single value parameter
    Singular,
    /// Variadic (star) parameter accepting multiple values
    Star,
}

/// Parse a justfile using `just --dump --dump-format json`.
pub async fn parse_justfile(path: &str) -> Result<Vec<ParsedRecipe>, String> {
    let out = Command::new("just")
        .args(["--dump", "--dump-format", "json", "--justfile", path])
        .output()
        .await
        .map_err(|e| format!("Failed to run just dump for {path}: {e}"))?;

    if !out.status.success() {
        return Err(format!(
            "just dump failed for {}: {}",
            path,
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    let dump: JustDump = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("Failed to parse just dump JSON for {path}: {e}"))?;

    let recipes = dump
        .recipes
        .into_iter()
        .map(|(name, r)| {
            let is_private = r.private || name.starts_with('_');
            let is_mcp_hidden = r
                .doc
                .as_ref()
                .map(|d| d.to_ascii_lowercase().contains("@mcp-hidden"))
                .unwrap_or(false);
            let params = r
                .parameters
                .into_iter()
                .map(|p| ParsedParam {
                    name: p.name,
                    kind: if p.kind == "star" || p.kind == "plus" {
                        ParamKind::Star
                    } else {
                        ParamKind::Singular
                    },
                    has_default: p.default.is_some(),
                })
                .collect();
            ParsedRecipe {
                name,
                doc: r.doc,
                is_private,
                is_mcp_hidden,
                params,
            }
        })
        .collect();

    Ok(recipes)
}

/// Parse a JSON dump string (for testing without the just binary).
pub fn parse_dump_json(json: &str) -> Result<Vec<ParsedRecipe>, String> {
    let dump: JustDump =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse JSON: {e}"))?;

    let recipes = dump
        .recipes
        .into_iter()
        .map(|(name, r)| {
            let is_private = r.private || name.starts_with('_');
            let is_mcp_hidden = r
                .doc
                .as_ref()
                .map(|d| d.to_ascii_lowercase().contains("@mcp-hidden"))
                .unwrap_or(false);
            let params = r
                .parameters
                .into_iter()
                .map(|p| ParsedParam {
                    name: p.name,
                    kind: if p.kind == "star" || p.kind == "plus" {
                        ParamKind::Star
                    } else {
                        ParamKind::Singular
                    },
                    has_default: p.default.is_some(),
                })
                .collect();
            ParsedRecipe {
                name,
                doc: r.doc,
                is_private,
                is_mcp_hidden,
                params,
            }
        })
        .collect();

    Ok(recipes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DUMP: &str = r#"{
        "recipes": {
            "build": {
                "doc": "Build the project",
                "private": false,
                "parameters": [
                    {"name": "target", "kind": "singular", "default": null}
                ]
            },
            "_internal": {
                "doc": null,
                "private": false,
                "parameters": []
            },
            "hidden": {
                "doc": "Do not use @mcp-hidden",
                "private": false,
                "parameters": []
            },
            "test": {
                "doc": "Run tests",
                "private": true,
                "parameters": [
                    {"name": "args", "kind": "star", "default": []}
                ]
            }
        }
    }"#;

    #[test]
    fn parses_recipes_from_json() {
        let recipes = parse_dump_json(SAMPLE_DUMP).unwrap();
        assert_eq!(recipes.len(), 4);
    }

    #[test]
    fn detects_private_by_attribute() {
        let recipes = parse_dump_json(SAMPLE_DUMP).unwrap();
        let test = recipes.iter().find(|r| r.name == "test").unwrap();
        assert!(test.is_private);
    }

    #[test]
    fn detects_private_by_underscore_prefix() {
        let recipes = parse_dump_json(SAMPLE_DUMP).unwrap();
        let internal = recipes.iter().find(|r| r.name == "_internal").unwrap();
        assert!(internal.is_private);
    }

    #[test]
    fn detects_mcp_hidden_annotation() {
        let recipes = parse_dump_json(SAMPLE_DUMP).unwrap();
        let hidden = recipes.iter().find(|r| r.name == "hidden").unwrap();
        assert!(hidden.is_mcp_hidden);
    }

    #[test]
    fn parses_star_parameters() {
        let recipes = parse_dump_json(SAMPLE_DUMP).unwrap();
        let test = recipes.iter().find(|r| r.name == "test").unwrap();
        assert_eq!(test.params.len(), 1);
        assert_eq!(test.params[0].kind, ParamKind::Star);
        assert!(test.params[0].has_default);
    }

    #[test]
    fn parses_singular_parameters() {
        let recipes = parse_dump_json(SAMPLE_DUMP).unwrap();
        let build = recipes.iter().find(|r| r.name == "build").unwrap();
        assert_eq!(build.params.len(), 1);
        assert_eq!(build.params[0].kind, ParamKind::Singular);
        assert!(!build.params[0].has_default);
    }
}
