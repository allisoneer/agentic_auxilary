# MCP Advanced Features Example

This example demonstrates advanced MCP (Model Context Protocol) features supported by the Universal Tool Framework.

## Overview

The example implements a comprehensive text processing tool suite showcasing:
- Basic text operations (analyze, transform)
- Progress reporting for long operations
- Cancellation support
- Tool annotations (read-only, destructive, idempotent)
- Rich error handling with MCP error codes

### Available Tools:
- `analyze_text`: Count words, lines, and characters in text
- `to_uppercase`: Convert text to uppercase
- `to_lowercase`: Convert text to lowercase
- `reverse_text`: Reverse the characters in text
- `summarize`: Extract a summary from text (read-only, idempotent)
- `clear_text`: Clear all text (destructive operation)
- `process_large_dataset`: Process data with progress reporting and cancellation
- `validate_text`: Validate text with detailed error messages

## Building and Running

### Build the example:
```bash
cargo build --example 05-mcp-basic
```

### Run the MCP server:
```bash
cargo run --example 05-mcp-basic
```

The server communicates via stdio (standard input/output) using the MCP protocol.

## Testing with MCP Clients

### Option 1: Using the Official Python SDK

Test the server using the official MCP Python SDK with uv:

```bash
# From the 05-mcp-basic directory
cd examples/05-mcp-basic
uv run test_with_uv.py
```

The test client will:
1. Start the MCP server
2. Initialize the connection using the official protocol
3. List available tools
4. Test each tool with sample data
5. Display the results

### Option 2: Using the Official TypeScript SDK

Test the server using the official MCP TypeScript SDK with bun:

```bash
# From the 05-mcp-basic directory
cd examples/05-mcp-basic

# First install the SDK (bun auto-install has issues with scoped packages)
bun add @modelcontextprotocol/sdk

# Then run the test
bun run test_with_bun_simple.ts
```

Both test clients handle the complete MCP handshake protocol correctly and demonstrate successful integration with rmcp v0.6.4.

### Option 3: Using Claude Desktop

You can integrate this MCP server with Claude Desktop by adding it to your configuration:

1. Build the example in release mode:
   ```bash
   cargo build --release --example 05-mcp-basic
   ```

2. Add to your Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):
   ```json
   {
     "mcpServers": {
       "utf-text-tools": {
         "command": "/path/to/universal_tool/target/release/examples/05-mcp-basic"
       }
     }
   }
   ```

3. Restart Claude Desktop to load the new MCP server.

### Option 4: Using mcp-client CLI

If you have the official MCP client installed:

```bash
# Install mcp-client (if not already installed)
npm install -g @modelcontextprotocol/client

# Connect to the server
mcp-client stdio -- cargo run --example 05-mcp-basic
```

## Advanced Features Demonstrated

### 1. **Progress Reporting**
The `process_large_dataset` tool shows how to report progress:
```rust
async fn process_large_dataset(
    &self,
    item_count: usize,
    delay_ms: Option<u64>,
    progress: Option<Box<dyn ProgressReporter>>,
    cancellation: CancellationToken,
) -> Result<ProcessingResult, ToolError>
```

UTF automatically handles progress token injection when provided by the MCP client.

### 2. **Cancellation Support**
Long-running operations can be cancelled using `CancellationToken`:
```rust
if cancellation.is_cancelled() {
    return Ok(ProcessingResult {
        processed_count: processed,
        cancelled: true,
        // ...
    });
}
```

### 3. **Tool Annotations**
Tools can specify MCP hints for better client behavior:
```rust
#[universal_tool(
    description = "Extract a summary from text",
    mcp(read_only_hint = true, idempotent_hint = true)
)]
```

Available annotations:
- `read_only_hint`: Tool only reads data
- `destructive_hint`: Tool performs destructive operations
- `idempotent_hint`: Same input produces same output
- `open_world_hint`: Tool accepts additional parameters

### 4. **Rich Error Handling**
UTF automatically converts `ToolError` to MCP error format:
```rust
return Err(ToolError::new(
    ErrorCode::InvalidArgument,
    "Text too short"
).with_details("Provide longer text"));
```

Error codes map to standard JSON-RPC error codes:
- `InvalidArgument` → `-32602` (Invalid params)
- `NotFound` → `-32002` (Resource not found)
- `Internal` → `-32603` (Internal error)

### 5. **Schema Generation**
UTF uses schemars for automatic JSON Schema generation:
```rust
#[derive(JsonSchema)]
struct ProcessingResult {
    #[schemars(description = "Number of items processed")]
    processed_count: usize,
}
```

## Implementation Details

The example showcases:

1. **Tool Definition**: Using `#[universal_tool]` attributes to define MCP-compatible tools
2. **Generated Methods**: UTF automatically generates:
   - `get_mcp_tools()` - Returns tool definitions for discovery
   - `handle_mcp_call()` - Dispatches tool calls to the appropriate methods
3. **Server Integration**: Using the `implement_mcp_server!` macro to create a complete MCP server
4. **Type Safety**: All parameters and return types are automatically serialized/deserialized

## Code Structure

```rust
// Define your tool struct
struct TextTools {
    name: String,
}

// Use the universal_tool_router macro
#[universal_tool_router]
impl TextTools {
    // Define tools with the universal_tool attribute
    #[universal_tool(description = "...")]
    async fn tool_name(&self, param: Type) -> Result<Output, ToolError> {
        // Implementation
    }
}

// Create the MCP server
struct TextToolsServer {
    tools: Arc<TextTools>,
}

// Use the macro to implement ServerHandler
implement_mcp_server!(TextToolsServer, tools);

// Run the server
server.serve(stdio()).await
```

## Troubleshooting

### Server doesn't start
- Check that you're in the correct directory
- Ensure all dependencies are installed: `cargo build`
- Check for any build errors

### Tools not appearing
- Verify the `#[universal_tool]` attributes are correctly applied
- Ensure the `#[universal_tool_router]` macro is on the impl block
- Check that methods are `async` and return `Result<T, ToolError>`

### Communication errors
- The server uses stdio for communication - ensure your client supports this
- Check that JSON-RPC messages are properly formatted
- Enable debug logging by setting `RUST_LOG=debug`

### MCP Protocol Handshake
The MCP protocol requires a specific 3-step handshake:
1. Client sends `initialize` request
2. Server responds with `InitializeResult`
3. Client sends `notifications/initialized` notification
4. Only then can normal operations (tools/list, tools/call) proceed

If you see errors like "expected initialized notification", ensure your client follows this handshake sequence.

## Next Steps

- Add more complex tools with multiple parameters
- Implement tools that interact with external services
- Add proper error handling and validation
- Explore MCP annotations for tool hints (read-only, destructive, etc.)