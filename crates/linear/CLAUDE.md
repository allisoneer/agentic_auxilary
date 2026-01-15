# CLAUDE.md - linear-tools

## Overview

Linear issue management tools providing MCP + CLI interfaces.

## Quick Commands

```bash
cd linear_tools && just check    # Format + clippy
cd linear_tools && just test     # Run tests only
cd linear_tools && just build    # Build the project
```

## Environment Variables

- `LINEAR_API_KEY` (required): Linear API key for authentication
- `LINEAR_GRAPHQL_URL` (optional): Override endpoint for testing
- `LINEAR_TOOLS_EXTRAS` (optional): Extra fields for MCP output (id,url,dates,assignee,state,team,priority)

## Architecture

3-crate structure following Cynic large-API pattern:
- `linear-schema`: Cached schema with rkyv optimization
- `linear-queries`: QueryFragments, InputObjects, scalar mappings
- `linear-tools`: Application with universal_tool router

## Testing

Integration tests use mockito for HTTP mocking. Tests that modify env vars use `serial_test`.

```bash
cargo test -p linear-tools -p linear-queries --all-features
```
