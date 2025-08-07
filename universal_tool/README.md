# Universal Tool Framework (UTF)

Write your tool logic once. Deploy it as a CLI, REST API, or MCP server—no code changes required.

## What is UTF?

UTF is a Rust code generation library that eliminates the boilerplate of exposing your business logic through different interfaces. Define your tools once with simple attributes, and UTF generates the integration code for command-line interfaces, REST APIs, and Model Context Protocol (MCP) servers.

UTF is a **library, not a framework**—it generates methods you call, never taking control of your application's runtime.

## Quick Start

```rust
use universal_tool_core::prelude::*;

#[universal_tool_router(
    cli(name = "calculator"),
    rest(prefix = "/api/v1"),
    mcp(name = "calculator-tools")
)]
impl Calculator {
    #[universal_tool(
        description = "Add two numbers",
        cli(name = "add"),
        rest(method = "POST", path = "/add")
    )]
    async fn add(&self, a: f64, b: f64) -> Result<f64, ToolError> {
        Ok(a + b)
    }
}

// That's it! UTF generates these methods:
// - create_cli_command() -> clap::Command
// - execute_cli(&self, matches) -> Result<(), ToolError>
// - create_rest_router(state) -> axum::Router
// - handle_mcp_call(&self, method, params) -> Result<Value, ToolError>
```

## Installation

```toml
[dependencies]
universal-tool = "0.1"

# Enable the interfaces you need
[features]
default = ["cli", "rest"]
cli = ["universal-tool/cli"]
rest = ["universal-tool/rest"]
mcp = ["universal-tool/mcp"]
```

## Features

- ✅ **Zero Runtime Overhead**: Pure code generation at compile time
- ✅ **Type Safety**: All parameters must implement `Serialize`, `Deserialize`, and `JsonSchema`
- ✅ **Unified Error Handling**: Consistent errors across all interfaces
- ✅ **Flexible Deployment**: Use one, two, or all three interfaces
- ✅ **Framework Agnostic**: Integrates with your existing application
- ✅ **Production Ready**: Built on battle-tested libraries (clap, axum, etc.)

## Examples

Explore our examples to see UTF in action:

1. **[CLI Simple](examples/01-cli-simple)** - Basic math operations exposed as CLI commands
2. **[CLI Advanced](examples/02-cli-advanced-params)** - Complex parameter handling and validation
3. **[REST Simple](examples/03-rest-simple)** - Basic REST API with CRUD operations
4. **[REST Idiomatic](examples/04-rest-idiomatic)** - Production-ready REST patterns
5. **[MCP Tools](examples/05-mcp-tools)** - IDE integration via Model Context Protocol
6. **[Kitchen Sink](examples/06-kitchen-sink)** - All interfaces in one application

## Documentation

- [Getting Started Guide](docs/getting-started.md) - Build your first UTF tool in 5 minutes
- [Architecture Overview](docs/architecture.md) - Understand how UTF works under the hood
- [Migration Guide](docs/migration.md) - Convert existing CLIs and APIs to UTF
- [API Reference](https://docs.rs/universal-tool) - Complete API documentation (available after crates.io publication)

## Why UTF?

### The Problem

You've built great business logic, but exposing it to users means:
- Writing CLI argument parsers with clap
- Setting up REST endpoints with axum
- Implementing MCP methods for using with LLMs/agents

Each interface requires different boilerplate, error handling, and parameter parsing. Changes to your logic mean updating code in multiple places.

### The UTF Solution

UTF eliminates this duplication through code generation. Your business logic stays clean and focused, while UTF handles:

- **CLI**: Automatic argument parsing, help text, and subcommands
- **REST**: Route registration, parameter extraction, and OpenAPI compatibility
- **MCP**: Method dispatch, JSON-RPC handling, and schema generation

### Philosophy

UTF follows these principles:

1. **Library, Not Framework**: UTF generates methods you call—it never owns your main() function
2. **Single Source of Truth**: Define tools once, expose them everywhere
3. **Zero Magic**: All generated code is straightforward and debuggable
4. **Type Safety First**: Compile-time validation prevents runtime errors
5. **Incremental Adoption**: Start with one interface, add others when needed

## Use Cases

UTF excels at:

- **Developer Tools**: Build CLIs that can also run as services
- **Internal APIs**: Expose the same operations via CLI for debugging and REST for integration
- **AI Tool Development**: Create MCP servers that can also be tested via CLI
- **Microservices**: Consistent interfaces across your service mesh
- **Migration Projects**: Gradually move from CLI-only to service-based architecture

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

```bash
# Clone the repository
git clone https://github.com/allisoneer/agentic_auxilary
cd agentic_auxilary/universal_tool

# Build the entire workspace
cargo build --all-features

# Run tests
cargo test --workspace

# Run examples
cargo run --example 01-cli-simple -- add 5 3
```

## License

MIT - See [LICENSE](../LICENSE) in the root of the repository.

## Acknowledgments

UTF builds upon excellent Rust libraries:
- [clap](https://github.com/clap-rs/clap) for CLI parsing
- [axum](https://github.com/tokio-rs/axum) for REST APIs
- [serde](https://github.com/serde-rs/serde) for serialization
- [schemars](https://github.com/GREsau/schemars) for JSON Schema generation
