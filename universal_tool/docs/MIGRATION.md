# Universal Tool Framework - MCP Migration Guide

## rmcp v0.2 to v0.6.4+ Migration

This guide covers the migration from rmcp v0.2 to v0.6.4+ (official rust-sdk) for the Universal Tool Framework's MCP functionality.

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

### Protocol Compliance

The newer rmcp version is stricter about MCP protocol compliance. Ensure your clients follow the required handshake sequence:

1. Client sends `initialize` request
2. Server responds with `InitializeResult`
3. Client sends `notifications/initialized` notification
4. Only then can normal operations proceed

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

3. **Update test clients:**
   Ensure all test clients send the `notifications/initialized` notification after receiving the initialize response.

4. **Test with official clients:**
   Test your server with official MCP clients like MCP Inspector to ensure protocol compliance.

### Backward Compatibility

Currently, there is no compatibility mode to support older non-compliant clients. All clients must follow the proper MCP handshake sequence when connecting to servers using rmcp v0.6.4+.

### Error Diagnostics

The updated framework includes improved error diagnostics. When handshake errors occur, helpful hints are displayed:

- "Client must send 'initialize' request first."
- "Client must send 'notifications/initialized' after receiving InitializeResult."

These hints help identify protocol compliance issues quickly.

### Testing

After migration, run the following to verify everything works:

```bash
# Build all features
cargo build --workspace --all-features

# Run tests
cargo test --workspace

# Build examples
cargo build --examples

# Test with official MCP clients
mcp-client stdio -- cargo run --example 05-mcp-basic
```