# pr_comments

A tool to fetch GitHub PR comments and reply to review comments via CLI and MCP interfaces.

## Installation

```bash
cd pr_comments
make install
```

## Usage

### CLI Mode

```bash
# Get review comments (thread-paginated, unresolved by default)
pr_comments comments --pr 123

# Include resolved review comments
pr_comments comments --pr 123 --include-resolved

# Filter by comment source (robot, human, or all)
pr_comments comments --pr 123 --comment-source-type robot

# Reply to a review comment (auto-prefixes with AI identifier)
pr_comments reply --comment-id 12345 --body "Thanks for the feedback!"

# List PRs
pr_comments list-prs --state open
```

### Thread-Level Pagination

Comments are paginated at the thread level (parent + all replies stay together). By default, 10 threads are returned per call. Repeated MCP calls with the same parameters return the next page.

Configure page size via environment variable:
```bash
export PR_COMMENTS_PAGE_SIZE=20
```

### Resolution Filtering

- Review comments can be part of conversation threads that can be marked as resolved
- By default, resolved comments are filtered out to focus on active discussions
- Use `--include-resolved` to see all comments including resolved ones

### Comment Source Filtering

Use `--comment-source-type` to filter by who created the comment:
- `robot` - Only comments from bots
- `human` - Only comments from humans
- `all` - All comments (default)

### MCP Mode

```bash
# Start MCP server
pr_comments mcp
```

**Available MCP tools:**
- `get_comments` - Get review comments with thread-level pagination
- `add_comment_reply` - Reply to a review comment (auto-prefixes with AI identifier)
- `list_prs` - List pull requests

## Authentication

Set the `GITHUB_TOKEN` or `GH_TOKEN` environment variable:

```bash
export GITHUB_TOKEN=your_github_token
```

Alternatively, the tool will use credentials from `gh auth login` if available.

Note: The token needs the `repo` scope for private repositories.

## Configuration

If not in a git repository, specify the repository:

```bash
pr_comments --repo owner/repo comments
```
