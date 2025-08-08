//! Integration tests for CLI code generation
//!
//! These tests verify that the generated CLI methods work correctly
//! and that all Phase 4 features are properly implemented.

use serde::{Deserialize, Serialize};
use universal_tool_core::prelude::*;
use universal_tool_macros::{universal_tool, universal_tool_router};

/// Test struct for basic CLI generation
struct TestTools;

#[universal_tool_router(cli(name = "test-cli", description = "Test CLI for integration tests"))]
impl TestTools {
    /// Basic add function
    #[universal_tool(description = "Add two numbers", cli(name = "add"))]
    async fn add(&self, a: i32, b: i32) -> Result<i32, ToolError> {
        Ok(a + b)
    }

    /// Function with aliases
    #[universal_tool(
        description = "Check if even",
        cli(name = "is-even", alias = "even", alias = "e")
    )]
    async fn is_even(&self, n: i32) -> Result<bool, ToolError> {
        Ok(n % 2 == 0)
    }

    /// Hidden function
    #[universal_tool(
        description = "Secret debug function",
        cli(name = "debug", hidden = true)
    )]
    async fn debug(&self) -> Result<String, ToolError> {
        Ok("debug output".to_string())
    }

    /// Function with environment variable support
    #[universal_tool(description = "Format with prefix", cli(name = "format"))]
    async fn format_text(
        &self,
        text: String,
        #[universal_tool_param(env = "TEST_PREFIX", default = ">>>")] prefix: Option<String>,
    ) -> Result<String, ToolError> {
        let prefix = prefix.unwrap_or_else(|| ">>>".to_string());
        Ok(format!("{} {}", prefix, text))
    }

    /// Function with optional parameter and default
    #[universal_tool(description = "Concat strings", cli(name = "concat"))]
    async fn concat(
        &self,
        a: String,
        b: String,
        #[universal_tool_param(default = " ", description = "Separator")] sep: Option<String>,
    ) -> Result<String, ToolError> {
        let sep = sep.unwrap_or_else(|| " ".to_string());
        Ok(format!("{}{}{}", a, sep, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_command_generation() {
        let tools = TestTools;
        let cmd = tools.create_cli_command();

        // Verify the command was created
        assert_eq!(cmd.get_name(), "test-cli");
        assert_eq!(
            cmd.get_about().map(|s| s.to_string()),
            Some("Test CLI for integration tests".to_string())
        );

        // Verify subcommands exist
        let subcommands: Vec<_> = cmd
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .collect();

        assert!(subcommands.contains(&"add".to_string()));
        assert!(subcommands.contains(&"is-even".to_string()));
        assert!(subcommands.contains(&"format".to_string()));
        assert!(subcommands.contains(&"concat".to_string()));

        // Debug command should be present but hidden
        assert!(subcommands.contains(&"debug".to_string()));
    }

    #[test]
    fn test_aliases() {
        let tools = TestTools;
        let cmd = tools.create_cli_command();

        // Find the is-even subcommand
        let is_even_cmd = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "is-even")
            .expect("is-even command not found");

        // Check aliases
        let aliases: Vec<_> = is_even_cmd
            .get_all_aliases()
            .map(|a| a.to_string())
            .collect();

        assert!(aliases.contains(&"even".to_string()));
        assert!(aliases.contains(&"e".to_string()));
    }

    #[test]
    fn test_hidden_command() {
        let tools = TestTools;
        let cmd = tools.create_cli_command();

        // Find the debug subcommand
        let debug_cmd = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "debug")
            .expect("debug command not found");

        // Check that it's hidden
        assert!(debug_cmd.is_hide_set());
    }

    #[test]
    fn test_environment_variable() {
        let tools = TestTools;
        let cmd = tools.create_cli_command();

        // Find the format subcommand
        let format_cmd = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "format")
            .expect("format command not found");

        // Find the prefix argument
        let prefix_arg = format_cmd
            .get_arguments()
            .find(|a| a.get_id() == "prefix")
            .expect("prefix argument not found");

        // Check environment variable is set
        let env_name = prefix_arg.get_env().expect("Environment variable not set");
        assert_eq!(env_name.to_str().unwrap(), "TEST_PREFIX");
    }

    #[test]
    fn test_default_values() {
        let tools = TestTools;
        let cmd = tools.create_cli_command();

        // Find the concat subcommand
        let concat_cmd = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "concat")
            .expect("concat command not found");

        // Find the separator argument
        let sep_arg = concat_cmd
            .get_arguments()
            .find(|a| a.get_id() == "sep")
            .expect("sep argument not found");

        // Check default value
        let default_values: Vec<_> = sep_arg
            .get_default_values()
            .into_iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(default_values, vec![" "]);
    }

    #[tokio::test]
    async fn test_execute_cli() {
        let tools = TestTools;

        // Test add command
        let app = tools.create_cli_command();
        let matches = app
            .clone()
            .try_get_matches_from(vec!["test-cli", "add", "--a", "5", "--b", "3"])
            .expect("Failed to parse add command");

        let result = tools.execute_cli(matches).await;
        assert!(result.is_ok());
    }
}
