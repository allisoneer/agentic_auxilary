# Example: Kitchen Sink - All Interfaces in One Application

## Overview

This example demonstrates the full power of the Universal Tool Framework by implementing a complete file management system that can be used as a CLI, REST API, or MCP server. It showcases how the same business logic can be exposed through all three interfaces without code duplication.

## Key Features

- **Complete File Management**: List, read, write, copy, delete files and directories
- **Text Search**: Search for text patterns across files with filtering
- **File Statistics**: Get insights about file system usage
- **Operation Logging**: Track all operations performed
- **Path Resolution**: Support for ~ home directory expansion
- **All Three Interfaces**: CLI, REST API, and MCP server in one binary
- **Feature Flags**: Conditional compilation for different deployment scenarios
- **Production Patterns**: Error handling, logging, middleware integration

## Running the Example

### As a CLI

```bash
# Build the example
cargo build --example 06-kitchen-sink

# List files in current directory
cargo run --example 06-kitchen-sink -- ls .

# List files recursively with hidden files
cargo run --example 06-kitchen-sink -- ls . --recursive --include-hidden

# Read a file
cargo run --example 06-kitchen-sink -- cat README.md

# Write a new file
cargo run --example 06-kitchen-sink -- write test.txt "Hello, UTF!"

# Search for text in files
cargo run --example 06-kitchen-sink -- grep "TODO" . --extensions "rs,toml"

# Get directory statistics
cargo run --example 06-kitchen-sink -- stats .

# View operation log
cargo run --example 06-kitchen-sink -- log

# Get help
cargo run --example 06-kitchen-sink -- --help
```

### As a REST API

```bash
# Start the REST API server
cargo run --example 06-kitchen-sink -- --rest --port 3000

# In another terminal:

# List files
curl "http://localhost:3000/api/v1/files?path=."

# Read a file
curl "http://localhost:3000/api/v1/files/content?path=README.md"

# Create a file
curl -X POST http://localhost:3000/api/v1/files \
  -H "Content-Type: application/json" \
  -d '{
    "path": "test.txt",
    "content": "Hello from REST API!",
    "overwrite": false
  }'

# Copy a file
curl -X POST http://localhost:3000/api/v1/files/copy \
  -H "Content-Type: application/json" \
  -d '{
    "source": "test.txt",
    "destination": "test-copy.txt"
  }'

# Delete a file
curl -X DELETE "http://localhost:3000/api/v1/files?path=test.txt"

# Search for text
curl "http://localhost:3000/api/v1/files/search?query=TODO&path=.&extensions=rs"

# Get statistics
curl "http://localhost:3000/api/v1/files/stats?path=."

# View operation log
curl "http://localhost:3000/api/v1/operations/log"
```

### As an MCP Server

```bash
# Run as MCP server (reads from stdin, writes to stdout)
cargo run --example 06-kitchen-sink -- --mcp

# Test with a simple client (see test scripts in the MCP example)
```

## Code Highlights

### Single Implementation, Three Interfaces

The core file management logic is implemented once and exposed through all interfaces:

```rust
#[universal_tool_router(
    cli(name = "filemanager"),
    rest(prefix = "/api/v1"),
    mcp(name = "file-manager")
)]
impl FileManager {
    #[universal_tool(
        description = "List files in a directory",
        cli(name = "ls"),
        rest(method = "GET", path = "/files")
    )]
    async fn list_files(
        &self,
        path: String,
        include_hidden: Option<bool>,
        recursive: Option<bool>,
    ) -> Result<DirectoryListing, ToolError> {
        // Implementation works for all three interfaces
    }
}
```

### Unified Error Handling

UTF's error types work consistently across all interfaces:

```rust
if !resolved_path.exists() {
    return Err(ToolError::not_found(format!("Path not found: {}", path)));
}
```

This error will be:
- Displayed as a CLI error message
- Returned as HTTP 404 in REST
- Formatted as a JSON-RPC error in MCP

### Runtime Interface Selection

The main function demonstrates how to choose interfaces at runtime:

```rust
if args.rest {
    // Run as REST API with middleware
    let app = file_manager.create_rest_router()
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());
    
    axum::serve(listener, app).await?;
} else if args.mcp {
    // Run as MCP server on stdio
    run_mcp_server(file_manager).await?;
} else {
    // Run as CLI
    let matches = file_manager.create_cli_command().get_matches();
    file_manager.execute_cli(&matches).await?;
}
```

### Complex Parameter Types

The example shows how UTF handles complex nested types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SearchResults {
    query: String,
    results: Vec<SearchResult>,
    total_matches: usize,
    files_searched: usize,
}
```

### State Management

The FileManager maintains state (operation log) that's accessible from all interfaces:

```rust
#[derive(Clone)]
struct FileManager {
    operation_log: Arc<Mutex<Vec<String>>>,
}
```

## Deployment Scenarios

### Scenario 1: Development Tool

Deploy as a CLI for local development:
```bash
cargo install --path . --example 06-kitchen-sink
filemanager ls ~/projects
```

### Scenario 2: Microservice

Deploy as a REST API in a container:
```dockerfile
FROM rust:slim
COPY . .
RUN cargo build --release --example 06-kitchen-sink
CMD ["./target/release/examples/06-kitchen-sink", "--rest"]
```

### Scenario 3: IDE Integration

Configure as an MCP server in your IDE:
```json
{
  "mcp-servers": {
    "file-manager": {
      "command": "cargo",
      "args": ["run", "--example", "06-kitchen-sink", "--", "--mcp"]
    }
  }
}
```

### Scenario 4: Feature-Flagged Builds

Build only the interfaces you need:

```toml
# Cargo.toml
[features]
default = ["cli"]
cli-only = ["universal-tool/cli"]
service = ["universal-tool/rest", "universal-tool/mcp"]
all = ["universal-tool/cli", "universal-tool/rest", "universal-tool/mcp"]
```

## Production Considerations

1. **Security**: Add authentication and authorization for REST/MCP interfaces
2. **Performance**: Implement streaming for large file operations
3. **Monitoring**: Integrate with observability tools using the logging framework
4. **Configuration**: Add configuration file support for default settings
5. **Testing**: Write integration tests for each interface

## Learn More

- [UTF Architecture Guide](../../docs/architecture.md)
- [CLI Example](../01-cli-simple/README.md)
- [REST Example](../04-rest-idiomatic/README.md)
- [MCP Example](../05-mcp-basic/README.md)
- [Getting Started Guide](../../docs/getting-started.md)