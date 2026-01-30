# coding_agent_tools

CLI + MCP tools for coding assistants. First tool: `ls` (gitignore-aware directory listing).

## Installation

```bash
cargo install --path .
```

## Usage

### CLI Mode

```bash
# List current directory (depth 1)
coding-agent-tools ls

# List with options
coding-agent-tools ls --path src --depth 2 --show files --hidden

# Add custom ignore patterns
coding-agent-tools ls --ignore "*.log" --ignore "tmp/"
```

### MCP Mode

```bash
coding-agent-tools mcp
```

Exposes the `ls` tool via MCP protocol for AI coding agents.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | string | `.` | Directory to list |
| `depth` | 0-10 | 1 | 0=header only, 1=children, 2+=tree |
| `show` | all/files/dirs | all | Filter by type |
| `ignore` | string[] | [] | Additional glob patterns to ignore |
| `hidden` | bool | false | Include hidden files |

## Features

- Gitignore-aware (respects .gitignore files)
- Built-in ignore patterns for common directories (node_modules, target, etc.)
- Implicit pagination for MCP (call again with same params for next page)
- Sorted output (directories first for `show=all`)
