# pr_comments

A tool to fetch GitHub PR comments via CLI and MCP interfaces. Automatically detects PRs from your current git branch or accepts manual PR specification.

## Installation

```bash
# From source
cd pr_comments
cargo install --path .

# After release (coming soon)
# Via homebrew or shell installer from GitHub releases
```

## GitHub Token Setup

Set your GitHub personal access token as an environment variable:

```bash
export GITHUB_TOKEN=your_github_token
```

### Required Permissions

- **Classic Personal Access Token**: `repo` scope
- **Fine-grained Personal Access Token**: `pull-requests: read` permission

## Usage

### CLI Mode

```bash
# Auto-detect PR from current branch and get all comments
pr_comments all

# Get all comments for specific PR
pr_comments all --pr 123

# Get only review comments (code comments)
pr_comments review-comments --pr 123

# Get only issue comments (discussion)
pr_comments issue-comments --pr 123

# List all PRs in the repository
pr_comments list-prs
pr_comments list-prs --state closed
pr_comments list-prs --state all
```

### MCP Mode

Start as an MCP server for AI assistants:

```bash
# Via flag
pr_comments --mcp

# Via subcommand
pr_comments mcp
```

#### Using with MCP Inspector

Test the MCP server with the official MCP Inspector:

```bash
# Install MCP Inspector (if not already installed)
npm install -g @modelcontextprotocol/inspector

# Connect to the pr_comments MCP server
mcp-inspector stdio -- pr_comments mcp
```

The MCP server follows the official protocol specification including the required 3-step handshake. Compatible with all official MCP clients.

### Specifying Repository

If not in a git repository, specify the repository explicitly:

```bash
pr_comments --repo owner/repo all
```

## Features

- **Auto-detection**: Automatically detects current PR from git branch
- **Dual Interface**: Works as both CLI tool and MCP server
- **Complete Coverage**: Fetches both review comments (code) and issue comments (discussion)
- **Pagination**: Handles large numbers of comments via API pagination
- **Type Safety**: Full TypeScript-style type definitions for MCP interface

## Development

```bash
# Check code
make check

# Run tests
make test

# Build
make build

# All checks
make all
```

## Architecture

The tool is built using:
- `octocrab` for GitHub API interactions
- `git2` for repository detection
- `universal_tool` framework for CLI/MCP interface generation
- Async Rust with Tokio runtime

## License

MIT