# GPT-5 Reasoner

Two-phase prompt optimization tool for GPT-5: optimize metadata with Claude, then execute with GPT-5.

## Features

- **Two-phase architecture**: Optimizer (configurable model) analyzes file metadata, executor (GPT-5) processes full content
- **Directory support**: Automatically discover and include files from directories with filtering
- **Dual interfaces**: CLI and MCP (Model Context Protocol) support via universal-tool framework
- **Smart file handling**: Binary detection, UTF-8 validation, path normalization
- **Configurable traversal**: Control recursion, hidden files, extensions, and file limits
- **Type safety**: Strongly-typed Rust with comprehensive test coverage (40 tests)

## Installation

```bash
cargo install gpt5_reasoner
```

Or build from source:
```bash
cd gpt5_reasoner
make build
```

## Usage

### CLI

Basic usage with files:
```bash
gpt5_reasoner run \
  --prompt "Analyze these files" \
  --prompt-type reasoning \
  --files-json examples/test_files.json
```

Using directories:
```bash
gpt5_reasoner run \
  --prompt "Analyze the codebase" \
  --prompt-type plan \
  --files-json examples/empty_files.json \
  --directories-json examples/directories.json
```

### MCP Server

Run as MCP server:
```bash
gpt5_reasoner mcp
```

### Environment Variables

- `OPENROUTER_API_KEY` (required): API key for OpenRouter
- `OPTIMIZER_MODEL` (optional): Override default optimizer model
  - Default: `anthropic/claude-sonnet-4.5`
  - Precedence: parameter > env var > default
- `RUST_LOG` (optional): Control logging level (e.g., `gpt5_reasoner=debug`)

## Directory Support

### DirectoryMeta Structure

```json
{
  "directory_path": "src",
  "description": "Source code files",
  "extensions": ["rs", "toml"],
  "recursive": true,
  "include_hidden": false,
  "max_files": 1000
}
```

### Fields

- **`directory_path`** (required): Path to directory (relative or absolute)
- **`description`** (required): Description inherited by all files from this directory
- **`extensions`** (optional): File extensions to include (case-insensitive, accepts "rs" or ".rs")
  - `null` or `[]`: Include all files
- **`recursive`** (optional, default: `false`): Traverse subdirectories
- **`include_hidden`** (optional, default: `false`): Include hidden files and directories
  - When `false`, prunes entire hidden directories from traversal
- **`max_files`** (optional, default: `1000`): Maximum files per directory (safety cap)

### Behavior Notes

- **Binary files**: Automatically skipped with debug logs
- **Extension matching**: Case-insensitive, handles both "rs" and ".rs" formats
- **Hidden directories**: When `include_hidden=false`, hidden directories are pruned entirely (not just files)
- **Path normalization**: All paths converted to absolute without resolving symlinks
- **Symlinks**: Not followed (`follow_links=false`)
- **Deduplication**: Files appearing in both `files` and `directories` are automatically deduplicated
- **Token limit**: Final guard enforces 250k token limit on complete prompt
- **Multi-dot extensions**: `foo.rs.bk` matches "bk", not "rs"
- **Platform**: Unix/Linux dotfile-based (Windows hidden attribute not checked)

### Examples

#### Recursive Rust files only
```json
[
  {
    "directory_path": "gpt5_reasoner/src",
    "description": "GPT-5 reasoner implementation",
    "extensions": ["rs"],
    "recursive": true,
    "include_hidden": false,
    "max_files": 1000
  }
]
```

#### All files including hidden
```json
[
  {
    "directory_path": "config",
    "description": "Configuration files",
    "recursive": false,
    "include_hidden": true
  }
]
```

#### Multiple directories
```json
[
  {
    "directory_path": "src",
    "description": "Source code",
    "extensions": ["rs"],
    "recursive": true,
    "include_hidden": false
  },
  {
    "directory_path": "tests",
    "description": "Test files",
    "extensions": ["rs"],
    "recursive": true,
    "include_hidden": false
  }
]
```

## Model Configuration

### Optimizer Model
The optimizer analyzes file metadata to determine which files to include and how to structure the final prompt.

**Default**: `anthropic/claude-sonnet-4.5`

Override via:
1. Function parameter (MCP/library usage)
2. `OPTIMIZER_MODEL` environment variable
3. Falls back to default

### Executor Model
The executor processes the full file content with the optimized prompt.

**Fixed**: `openai/gpt-5` (high reasoning effort)

This model is not configurable and always uses `reasoning_effort: high`.

### Reasoning Effort
- Models containing "gpt-5" or "gpt-oss" get `reasoning_effort` set
- Anthropic models do not support this parameter (correctly omitted)

## Architecture

```
User Prompt + File Metadata
         ↓
    Optimizer (Claude)
         ↓
   Optimized Prompt Template
         ↓
  File Content Injection
         ↓
   Executor (GPT-5 high reasoning)
         ↓
      Final Output
```

### Two-Phase Benefits
1. **Cost efficiency**: Optimizer sees only metadata (filenames, descriptions), not full content
2. **Token optimization**: Executor prompt is pre-filtered to relevant files only
3. **Flexibility**: Easy to swap optimizer models without changing executor

## Development

```bash
# Run checks
make check

# Run tests
make test

# Build
make build

# All in one
make all
```

## Breaking Changes from 0.1.0

- Default optimizer model changed from `openai/gpt-5` to `anthropic/claude-sonnet-4.5`
- `gpt5_reasoner_impl` signature now includes `directories` parameter
- `optimize_and_execute` MCP function now accepts optional `directories` parameter

## License

MIT - See [LICENSE](../LICENSE)
