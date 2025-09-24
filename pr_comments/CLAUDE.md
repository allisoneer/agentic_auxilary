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