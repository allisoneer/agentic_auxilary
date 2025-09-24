# pr_comments

A tool to fetch GitHub PR comments via CLI and MCP interfaces.

## Installation

```bash
cd pr_comments
make install
```

## Usage

### CLI Mode

```bash
# Auto-detect PR from current branch
pr_comments all

# Specify PR number
pr_comments all --pr 123

# Get only unresolved review comments (default behavior)
pr_comments review-comments --pr 123

# Include resolved review comments
pr_comments review-comments --pr 123 --include-resolved

# Get only issue comments (resolution doesn't apply)
pr_comments issue-comments --pr 123

# List PRs
pr_comments list-prs --state open
```

### Resolution Filtering (Breaking Change)

**Important**: As of v0.2.0, `review-comments` now defaults to showing only unresolved comments. This is a breaking change from previous versions.

- Review comments can be part of conversation threads that can be marked as resolved
- By default, resolved comments are filtered out to focus on active discussions
- Use `--include-resolved` to see all comments including resolved ones
- This filtering only applies to the `review-comments` command
- The `all` command always shows all comments regardless of resolution status

### MCP Mode

```bash
# Start MCP server
pr_comments --mcp

# Or
pr_comments mcp
```

## Authentication

Set the `GITHUB_TOKEN` environment variable:

```bash
export GITHUB_TOKEN=your_github_token
```

Note: The token needs the `repo` scope for private repositories.

## Configuration

If not in a git repository, specify the repository:

```bash
pr_comments --repo owner/repo all
```

## Comment Types

- **Review Comments**: Code-specific inline comments on the diff. These can be resolved.
- **Issue Comments**: General discussion comments on the PR. These cannot be resolved.