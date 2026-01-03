# CLAUDE.md - pr_comments Tool

## Overview
The pr_comments tool fetches GitHub PR review comments with support for filtering by resolution status and comment source type, and can reply to review comments.

## Key Implementation Details
- Uses hybrid REST + GraphQL approach for resolution filtering
- Default behavior: show only unresolved comments
- Supports bot detection via `is_bot` field on comments
- Thread-level pagination (parent + replies stay together)
- Replies are auto-prefixed with AI identifier

## Available Tools (3 total)

| Tool | Description |
|------|-------------|
| `get_comments` | Get PR review comments with thread-level pagination |
| `add_comment_reply` | Reply to a review comment (auto-prefixes with AI identifier) |
| `list_prs` | List pull requests in the repository |

## Common Commands
```bash
# Run tests
just test

# Check for linting issues
just check

# Build the tool
just build

# Format code
just fmt
```

## CLI Usage Examples
```bash
# Get review comments (thread-paginated, unresolved by default)
pr_comments comments --pr 123

# Filter by comment source (robot, human, or all)
pr_comments comments --pr 123 --comment-source-type robot

# Include resolved review comments
pr_comments comments --pr 123 --include-resolved

# Reply to a review comment
pr_comments reply --comment-id 12345 --body "Thanks for the feedback!"

# List PRs
pr_comments list-prs --state open
```

## Pagination

Comments are paginated at the thread level. Default page size is 10 threads. Configure via:
```bash
export PR_COMMENTS_PAGE_SIZE=20
```

In MCP mode, repeated calls with the same parameters return the next page. Cache expires after 5 minutes.

## Architecture Notes
- GraphQL query runs only when filtering resolved comments (performance optimization)
- Comment IDs are matched between REST and GraphQL APIs via database_id field
- Pagination cache uses 5-minute TTL with two-level locking
- If a comment isn't found in the GraphQL response, it's included by default (fail-open)

## MCP Text Output

The pr_comments MCP tool supports token-efficient text formatting. Set `PR_COMMENTS_EXTRAS` to include optional fields (comma-separated):

**Supported flags:**
- `id` or `ids`: Include numeric comment IDs (default: ON)
- `noid` or `no_ids`: Disable numeric comment IDs
- `url` or `urls`: Include HTML URLs
- `date`, `dates`, `time`, or `times`: Include created_at/updated_at timestamps
- `review`, `review_id`, or `review_ids`: Include pull_request_review_id for review comments
- `count` or `counts`: Include comment_count and review_comment_count (for PR list)
- `author` or `authors`: Include PR author (for PR list)

**Example:**
```bash
export PR_COMMENTS_EXTRAS="id,url,dates,review,counts,author"
```

**Default behavior** (no env var set): Minimal output with essential fields (user, path, line, body) plus comment IDs.

### MCP Tool Output Formats

**get_comments**: Returns grouped review comments by file path with format:
```
Review comments:
Legend: L = old (LEFT), R = new (RIGHT), - = unknown

src/lib.rs
  [12 R] alice #12345678
    Can you add error handling here?
    ↳ [12 R] bob #12345680
      Done, added Result<>
  [42 L] charlie #12345681
    This should use Result<>
```

**add_comment_reply**: Returns the created comment:
```
Reply posted:
src/lib.rs [12 R] ai-bot #22334455
  AI response: Thanks for the feedback!
```

**list_prs**: Returns compact PR list:
```
Pull requests:
#683 open — Implement llm2 framework
#682 closed — Fix schema validation bug
```

With `PR_COMMENTS_EXTRAS="author,counts"`:
```
Pull requests:
#683 open — Implement llm2 framework (by alice) [comments=4, review_comments=11]
#682 closed — Fix schema validation bug (by bob) [comments=2, review_comments=3]
```

### Token Reduction

Text formatting achieves **50-65% token reduction** compared to JSON by:
1. Eliminating repeated JSON keys
2. Grouping review comments by file path
3. Compressing side field (LEFT/RIGHT → L/R)
4. Making optional fields truly optional (controlled by env var)

**Comment bodies are never truncated** - all content is preserved in full.

### CLI and REST Behavior

CLI returns JSON for programmatic use. MCP interface returns formatted text for token efficiency.
