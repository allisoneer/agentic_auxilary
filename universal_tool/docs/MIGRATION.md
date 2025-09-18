# Universal Tool Framework - MCP Migration Guide

## rmcp v0.2 to v0.6.4+ Migration

This guide covers the migration from rmcp v0.2 to v0.6.4+ (official rust-sdk) for the Universal Tool Framework's MCP functionality.

### ⚠️ Critical Changes Summary

1. **Server must use `service.waiting().await`** - Without this, servers exit immediately
2. **Custom stdio test clients no longer work** - Use official SDKs instead
3. **Stricter connection handling** - Protocol violations result in immediate connection closure
4. **New required struct fields** - `Tool` and `Implementation` need additional fields

### Breaking Changes

#### 1. Error Type Changes
The `rmcp::Error` type has been deprecated and replaced with `rmcp::ErrorData`:

```rust
// Before (v0.2)
pub use rmcp::Error as McpError;

// After (v0.6.4+)
pub use rmcp::ErrorData as McpError;
```

#### 2. Tool Structure Changes
The `Tool` struct now requires additional fields:

```rust
// Before (v0.2)
Tool {
    name: name.into(),
    description: description.map(|s| s.into()),
    input_schema: input_schema.unwrap_or_else(|| Arc::new(serde_json::Map::new())),
    annotations,
}

// After (v0.6.4+)
Tool {
    name: name.clone().into(),
    title: name.into(),  // New required field
    description: description.map(|s| s.into()),
    input_schema: input_schema.unwrap_or_else(|| Arc::new(serde_json::Map::new())),
    annotations,
    output_schema: None,  // New field
    icons: None,          // New field
}
```

#### 3. Implementation Structure Changes
The `Implementation` struct (used in InitializeResult) now requires additional fields:

```rust
// Before (v0.2)
Implementation {
    name: "server-name".to_string().into(),
    version: "1.0.0".to_string().into(),
}

// After (v0.6.4+)
Implementation {
    name: "server-name".to_string().into(),
    title: "Server Title".to_string().into(),    // New required field
    version: "1.0.0".to_string().into(),
    website_url: None,                           // New field
    icons: None,                                 // New field
}
```

#### 4. Schema Generation Changes
The schemars module path has changed:

```rust
// Before (v0.2)
let settings = schemars::r#gen::SchemaSettings::draft07();

// After (v0.6.4+)
let settings = ::schemars::r#gen::SchemaSettings::draft07();
```

### Protocol Compliance and Connection Handling

The newer rmcp version has significantly stricter connection handling and protocol compliance:

#### Required Handshake Sequence:
1. Client sends `initialize` request
2. Server responds with `InitializeResult`
3. Client sends `notifications/initialized` notification
4. Only then can normal operations proceed

#### Critical Server Lifecycle Change:
**IMPORTANT**: Servers must now use the `waiting()` pattern:

```rust
// Before (v0.2) - This worked but is incomplete in v0.6.4
server.serve(transport).await?;

// After (v0.6.4+) - REQUIRED pattern
let service = server.serve(transport).await?;
service.waiting().await?;  // <-- Without this, the server exits immediately
```

Without `service.waiting().await`, your server will exit immediately after initialization.

Complete example:
```rust
use universal_tool_core::mcp::{ServiceExt, stdio};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = MyMcpServer::new();
    let transport = stdio();

    // CRITICAL: Must capture service and call waiting()
    let service = server.serve(transport).await?;
    service.waiting().await?;

    Ok(())
}
```

### Migration Steps

1. **Update Cargo.toml dependency:**
   ```toml
   # Before
   rmcp = { version = "0.2", features = ["server", "transport-io"], optional = true }

   # After
   rmcp = { version = "0.6.4", features = ["server", "transport-io"], optional = true }
   ```

2. **Fix compilation errors:**
   - Replace `rmcp::Error` with `rmcp::ErrorData`
   - Add required fields to `Tool` and `Implementation` structs
   - Update namespace references for schemars

3. **Update server implementation:**
   Add the `service.waiting().await` pattern to your server's main function.

4. **Replace custom test clients:**
   **IMPORTANT**: Custom stdio test clients that worked with rmcp v0.2 will NOT work with v0.6.4 due to stricter connection handling. The server will immediately close connections with "connection closed: initialized request" errors.

   Instead, use official MCP SDKs for testing:

   ```bash
   # Python SDK test (with uv)
   cd examples/05-mcp-basic
   uv run test_with_uv.py

   # TypeScript SDK test (with bun)
   cd examples/05-mcp-basic
   # Note: bun auto-install has issues with scoped packages, explicit install required
   bun add @modelcontextprotocol/sdk
   bun run test_with_bun_simple.ts
   ```

5. **Test with official clients:**
   - MCP Inspector
   - Official Python SDK (`mcp` package)
   - Official TypeScript SDK (`@modelcontextprotocol/sdk`)

   These clients properly implement the required handshake and connection handling.

### Backward Compatibility

Currently, there is no compatibility mode to support older non-compliant clients. All clients must follow the proper MCP handshake sequence when connecting to servers using rmcp v0.6.4+.

### Error Diagnostics

The updated framework includes improved error diagnostics. When handshake errors occur, helpful hints are displayed:

- "Client must send 'initialize' request first."
- "Client must send 'notifications/initialized' after receiving InitializeResult."

These hints help identify protocol compliance issues quickly.

### Common Issues and Troubleshooting

#### "connection closed: initialized request" Error
This error occurs when:
- Using custom stdio test clients from rmcp v0.2 era
- Missing the `service.waiting().await` pattern
- Client doesn't properly implement the MCP handshake

**Solution**: Use official SDKs and ensure your server includes `service.waiting().await`.

#### Server Exits Immediately
If your server starts and immediately exits without errors:
- You're missing `service.waiting().await` after `server.serve(transport).await`

#### Test Clients Fail to Connect
Custom test clients that manually construct JSON-RPC messages will fail with rmcp v0.6.4. The new version has stricter connection lifecycle management that immediately closes connections if the protocol isn't followed exactly.

### Testing

After migration, run the following to verify everything works:

```bash
# Build all features
cargo build --workspace --all-features

# Run tests
cargo test --workspace

# Build examples
cargo build --examples

# Test with official Python SDK
cd examples/05-mcp-basic
uv run test_with_uv.py

# Test with official TypeScript SDK
cd examples/05-mcp-basic
bun add @modelcontextprotocol/sdk
bun run test_with_bun_simple.ts

# Test with MCP Inspector (if installed)
mcp-client stdio -- cargo run --example 05-mcp-basic
```