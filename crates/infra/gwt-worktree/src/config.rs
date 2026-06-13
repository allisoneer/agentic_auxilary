use crate::command::CommandSpec;
use crate::error::Error;
use crate::error::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use toml::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GwtConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_repo: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repos: BTreeMap<String, RepoConfig>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post_create_commands: Vec<CommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clean_command: Option<CommandSpec>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl GwtConfig {
    pub fn load() -> Result<Self> {
        Self::load_from_path(&config_path()?)
    }

    pub fn load_or_create_default() -> Result<Self> {
        Self::load_or_create_default_at(&config_path()?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to_path(&config_path()?)
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)?;
        if contents.trim().is_empty() {
            return Ok(Self::default());
        }

        Ok(toml::from_str(&contents)?)
    }

    pub fn load_or_create_default_at(path: &Path) -> Result<Self> {
        if path.exists() {
            return Self::load_from_path(path);
        }

        let config = Self::default();
        config.save_to_path(path)?;
        Ok(config)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        let Some(parent) = path.parent() else {
            return Err(Error::Io(std::io::Error::other(
                "config path has no parent directory",
            )));
        };

        std::fs::create_dir_all(parent)?;
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|path| path.join("gwt"))
        .ok_or(Error::ConfigDirectoryUnavailable)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const LIVE_SHAPE: &str = r#"[repos."/home/allison/general_wisdom/monorepo/.git"]
post_create_commands = [
    "just setup",
    "thoughts init",
    "echo 'Worktree setup complete!'",
]

[repos."/home/allison/git/agentic_auxilary/.git"]
post_create_commands = [
    "thoughts init",
    "echo 'Worktree setup complete!'",
]
"#;

    #[test]
    fn load_missing_file_is_side_effect_free() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");

        let config = GwtConfig::load_from_path(&path).unwrap();

        assert_eq!(config, GwtConfig::default());
        assert!(!path.exists());
    }

    #[test]
    fn load_or_create_default_writes_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("gwt").join("config.toml");

        let config = GwtConfig::load_or_create_default_at(&path).unwrap();

        assert_eq!(config, GwtConfig::default());
        assert!(path.exists());
        let saved = std::fs::read_to_string(path).unwrap();
        assert!(!saved.contains("default_repo"));
    }

    #[test]
    fn omits_default_repo_when_unset() {
        let config = GwtConfig::default();

        let serialized = toml::to_string_pretty(&config).unwrap();

        assert!(!serialized.contains("default_repo"));
    }

    #[test]
    fn live_shape_round_trips() {
        let config: GwtConfig = toml::from_str(LIVE_SHAPE).unwrap();

        assert_eq!(config.default_repo, None);
        assert!(
            config
                .repos
                .contains_key("/home/allison/general_wisdom/monorepo/.git")
        );
        assert!(
            config
                .repos
                .contains_key("/home/allison/git/agentic_auxilary/.git")
        );

        let serialized = toml::to_string_pretty(&config).unwrap();
        let reparsed: Value = serialized.parse::<Value>().unwrap();
        let original: Value = LIVE_SHAPE.parse::<Value>().unwrap();
        assert_eq!(reparsed, original);
    }

    #[test]
    fn preserves_unknown_keys() {
        let input = r#"default_repo = "/tmp/repo/.git"
custom_root = "root-value"

[repos."/tmp/repo/.git"]
post_create_commands = ["thoughts init"]
custom_repo_flag = true
"#;

        let config: GwtConfig = toml::from_str(input).unwrap();
        let round_trip = toml::to_string_pretty(&config).unwrap();
        let reparsed: Value = round_trip.parse::<Value>().unwrap();
        let original: Value = input.parse::<Value>().unwrap();

        assert_eq!(reparsed, original);
    }
}
