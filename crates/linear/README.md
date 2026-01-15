# linear-tools

CLI + MCP tools for Linear issue management.

## Requirements

- `LINEAR_API_KEY` environment variable set to your Linear API key

## Usage

### CLI

```bash
# Search issues
linear-tools search --query "bug"

# Read a specific issue
linear-tools read --issue ENG-245
```

### MCP Server

```bash
linear-tools mcp
```

## Environment Variables

- `LINEAR_API_KEY` (required): Your Linear API key
- `LINEAR_GRAPHQL_URL` (optional): Override GraphQL endpoint (for testing)
- `LINEAR_TOOLS_EXTRAS` (optional): Comma-separated list of extra fields to show:
  - `id`, `url`, `dates`, `assignee`, `state`, `team`, `priority`
