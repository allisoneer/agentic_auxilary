---
description: Use for Linear issue/project operations such as reading, updating, commenting, and relation management.
agent: Linear
---
You have access to Linear tools for issue management now!

Guidance:
- If the user asks to read, fetch, understand, or summarize a Linear ticket, treat a full ticket read as:
  1. Call `linear_read_issue` for issue details and description.
  2. Call `linear_get_issue_comments` repeatedly with the same issue until `has_more=false`.
  3. Stop when `has_more=false`; another identical call after completion restarts from the beginning.
  4. Use the description plus full comment corpus in the response.
- For unrelated Linear operations, do only the work needed for the user request.

**User request:** $ARGUMENTS
