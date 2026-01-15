# Example: CLI Advanced Parameters

## Overview

This example demonstrates UTF's advanced CLI capabilities including complex parameter types, multiple output formats, progress indicators, and shell completion generation. It showcases how UTF handles sophisticated CLI requirements while maintaining clean, declarative code.

## Key Features

- **Vec<T> Parameters**: Accept multiple values for a single parameter
- **HashMap Parameters**: Key-value pair configuration options
- **Complex Types**: Structured parameters with nested fields
- **Output Formats**: JSON, YAML, table, and plain text output
- **Progress Indicators**: Simulated progress bars and spinners
- **Shell Completions**: Generate completion scripts for multiple shells
- **Custom Formatting**: Implement CliFormatter trait for rich output
- **Hidden Commands**: Commands that don't appear in help text

## Running the Example

### Basic Usage

```bash
# Build the example
cargo build --example 02-cli-advanced-params

# Get help
cargo run --example 02-cli-advanced-params -- --help

# Analyze multiple files
cargo run --example 02-cli-advanced-params -- analyze file1.txt file2.rs dir1/ --extensions rs,toml --show-progress

# Batch process items
cargo run --example 02-cli-advanced-params -- batch item1 item2 item3 --mode fast --fail-fast

# Process with configuration
cargo run --example 02-cli-advanced-params -- process-config --config key1=value1 --config key2=value2 --settings count=10 --settings limit=100

# Filter files with complex configuration
cargo run --example 02-cli-advanced-params -- filter --filter.include-patterns "*.rs" --filter.exclude-patterns "test*" --filter.min-size 1024 --directories src/ tests/
```

### Output Formats

UTF supports multiple output formats through the `--format` flag:

```bash
# JSON output
cargo run --example 02-cli-advanced-params -- analyze . --format json

# YAML output
cargo run --example 02-cli-advanced-params -- analyze . --format yaml

# Table output (default for some commands)
cargo run --example 02-cli-advanced-params -- analyze . --format table

# Plain text output
cargo run --example 02-cli-advanced-params -- analyze . --format text
```

### Shell Completions

Generate shell completion scripts:

```bash
# Bash completions
cargo run --example 02-cli-advanced-params -- completions bash > data-tools.bash

# Zsh completions
cargo run --example 02-cli-advanced-params -- completions zsh > _data-tools

# Fish completions
cargo run --example 02-cli-advanced-params -- completions fish > data-tools.fish

# PowerShell completions
cargo run --example 02-cli-advanced-params -- completions powershell > data-tools.ps1
```

## Code Highlights

### Vec<T> Parameters

UTF automatically handles multiple values:

```rust
#[universal_tool(description = "Analyze multiple files")]
async fn analyze_files(
    &self,
    paths: Vec<String>,        // Accepts multiple file paths
    extensions: Vec<String>,   // Accepts multiple extensions
) -> Result<AnalysisResult, ToolError>
```

CLI usage:
```bash
cargo run -- analyze file1.txt file2.txt file3.txt --extensions rs toml md
```

### HashMap Parameters

For key-value configuration:

```rust
#[universal_tool(description = "Process with configuration")]
async fn process_with_config(
    &self,
    config: HashMap<String, String>,  // String key-value pairs
    settings: HashMap<String, i32>,   // Numeric settings
) -> Result<ConfigResult, ToolError>
```

CLI usage:
```bash
cargo run -- process-config --config db=postgres --config cache=redis --settings timeout=30
```

### Complex Structured Types

UTF handles nested structures:

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct FilterConfig {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    min_size: Option<u64>,
    max_size: Option<u64>,
}

#[universal_tool(description = "Filter files")]
async fn filter_files(
    &self,
    filter: FilterConfig,
) -> Result<FilterResult, ToolError>
```

CLI usage:
```bash
cargo run -- filter \
  --filter.include-patterns "*.rs" "*.toml" \
  --filter.exclude-patterns "target/*" \
  --filter.min-size 100 \
  --filter.max-size 10000
```

### Custom Output Formatting

Implement the CliFormatter trait for rich output:

```rust
impl CliFormatter for AnalysisResult {
    fn format_text(&self) -> String {
        // Custom text formatting
    }
    
    fn format_table(&self) -> Vec<Vec<String>> {
        // Table representation
    }
}
```

### Progress Indicators

The example demonstrates where progress indicators would be integrated:

```rust
// TODO: ProgressReporter should be injected by the framework
// based on cli(progress_style = "bar")
for (i, path) in paths.iter().enumerate() {
    // Progress would be reported here if injected by framework
    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

### Hidden Commands

Commands can be hidden from help text:

```rust
#[universal_tool(
    description = "Generate shell completions",
    cli(name = "completions", hidden = true)
)]
```

## Advanced Patterns

### 1. Parameter Validation

UTF allows validation in the tool method:

```rust
if extensions.is_empty() {
    return Err(ToolError::invalid_input("At least one extension required"));
}
```

### 2. Conditional Logic

Use parameter values to control behavior:

```rust
if show_progress {
    // Show progress bar
} else {
    // Silent processing
}
```

### 3. Global Flags

The example shows handling of global flags like `--quiet` and `--verbose`:

```rust
let quiet = matches.get_flag("quiet");
let verbose = matches.get_count("verbose");
```

## Testing

The example includes tests demonstrating:
- Unit testing of tool methods
- Output format testing
- Parameter handling verification

```rust
#[tokio::test]
async fn test_analyze_files() {
    let tools = DataTools::new();
    let result = tools.analyze_files(
        vec!["test.txt".to_string()],
        vec![],
        false
    ).await.unwrap();
    
    assert_eq!(result.files_processed, 1);
}
```

## Production Considerations

1. **Progress Reporting**: In production, implement actual progress bars using indicatif or similar
2. **Large Collections**: Consider streaming for very large Vec parameters
3. **Validation**: Add comprehensive parameter validation
4. **Help Text**: Use parameter descriptions for better user experience
5. **Default Values**: Consider sensible defaults for optional parameters

## Learn More

- [CLI Simple Example](../01-cli-simple/README.md)
- [UTF CLI Documentation](../../docs/cli-interface.md)
- [Kitchen Sink Example](../06-kitchen-sink/README.md) - All interfaces combined