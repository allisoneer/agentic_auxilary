# CLAUDE.md - pr_comments Tool

## Overview
The pr_comments tool fetches GitHub PR comments with support for filtering by resolution status on review comments.

## Key Implementation Details
- Uses hybrid REST + GraphQL approach for resolution filtering
- Only `get_review_comments` supports resolution filtering
- Default behavior: show only unresolved comments (breaking change in v0.2.0)
- `get_all_comments` always includes all review comments regardless of resolution

## Common Commands
```bash
# Run tests
make test

# Check for linting issues
make check

# Build the tool
make build

# Install locally
make install
```

## Testing Resolution Filtering
```bash
# Test with a PR that has resolved comments
pr_comments review-comments --pr 123  # Shows only unresolved
pr_comments review-comments --pr 123 --include-resolved  # Shows all
pr_comments all --pr 123  # Shows all comments (no filtering)
```

## Architecture Notes
- GraphQL query runs only when filtering resolved comments (performance optimization)
- Comment IDs are matched between REST and GraphQL APIs via database_id field
- Pagination is handled for both REST and GraphQL queries
- If a comment isn't found in the GraphQL response, it's included by default (fail-open)

## MCP Text Output

The pr_comments MCP tool now supports token-efficient text formatting. Set `PR_COMMENTS_EXTRAS` to include optional fields (comma-separated):

**Supported flags:**
- `id` or `ids`: Include numeric comment/PR IDs
- `url` or `urls`: Include HTML URLs
- `date`, `dates`, `time`, or `times`: Include created_at/updated_at timestamps
- `review`, `review_id`, or `review_ids`: Include pull_request_review_id for review comments
- `count` or `counts`: Include comment_count and review_comment_count (for PR list)
- `author` or `authors`: Include PR author (for PR list)

**Example:**
```bash
export PR_COMMENTS_EXTRAS="id,url,dates,review,counts,author"
```

**Default behavior** (no env var set): Minimal output with only essential fields (user, path, line, body).

### MCP Tool Output Formats

**get_all_comments**: Returns PR header with totals, grouped review comments (by file path with legend), then issue comments.

**get_review_comments**: Returns grouped review comments by file path with format:
```
Review comments:
Legend: L = old (LEFT), R = new (RIGHT), - = unknown

src/lib.rs
  [12 R] alice
    Can you add error handling here?
  [42 L] bob
    This should use Result<>
```

**get_issue_comments**: Returns issue comments with format:
```
Issue comments:
  alice
    LGTM, merging soon
  bob
    Wait, we need to address the validation issue first
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

CLI and REST interfaces continue to return JSON. For CLI, list endpoints (review-comments, issue-comments, list-prs) return JSON arrays to maintain backward compatibility.

MCP interface returns formatted text for token efficiency.