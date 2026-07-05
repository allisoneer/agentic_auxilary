---
description: Use for Discord message search within the configured guild (read-only).
agent: Discord
---
You have access to Discord search tools now!

Guidance:
- Use `discord_search_messages` to full-text search the configured guild.
- Optional narrowing: `channel_id` and `author_id`.
- Pagination: use `offset` (0-based) and `limit` (1-25). The tool clamps values and returns `next_offset`.
- If you get an indexing error (HTTP 202), retry in a few seconds.

**User request:** $ARGUMENTS
