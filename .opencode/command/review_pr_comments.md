---
description: Review PR comments — triage, analyze with sub-agents, draft replies
---

<task>
Triage PR review comments and perform targeted deep analysis using sub-agents.

Default behavior (no args):
- Fetch all unresolved comments on the current PR
- Triage all threads (actionable, question, context, nit) with severity
- Spawn 1 sub-agent per actionable/question thread for deep analysis
- Write a versioned artifact preserving original threads and analysis
- Present reply drafts in the assistant response (not in artifact)

The user can then say "send reply for X" to post a specific reply.

Keep instructions LEAN — you already understand tool mechanics from their descriptions.
</task>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step name="interpret_intent" id="1">

## Interpret Intent (Free-text)

Infer user intent from `$ARGUMENTS`. Apply smart defaults if unspecified:
- Scope: current PR, all unresolved comments, comment_source_type = all
- Actions: triage all threads; analyze actionable + question threads
- Output: versioned artifact + reply drafts in response

Recognize common intents:
- "just triage" → skip deep analysis
- "analyze everything" → analyze all categories including context/nit
- "include resolved" → include resolved threads
- "humans only" / "robots only" → filter by comment_source_type
- "pr 123" → target specific PR
- "only questions" / "only actionable" → filter categories for analysis
- "skip nits" → exclude nit category from analysis
- "high severity only" → analyze only high-severity threads

Only ask a clarifying question if intent is truly ambiguous. Otherwise proceed with defaults.

When making assumptions (e.g., auto-detected PR, default filters), state them explicitly in your response so the user knows what scope is being analyzed.

</step>

<step name="identify_pr" id="2">

## Identify the PR

Prefer auto-detection: call `pr_comments_get_comments` without pr_number — the tool infers from current branch.

If auto-detection fails or user specified a PR number, use that.

If still unknown, call `pr_comments_list_prs` and ask user to select. State assumptions clearly.

</step>

<step name="fetch_comments" id="3">

## Fetch All Comment Threads (Paginated)

Use `pr_comments_get_comments` with:
- comment_source_type from intent ("all" default)
- include_resolved from intent (false default)
- pr_number from Step 2

Handle pagination: repeat calls with same params until no more threads returned.

For each thread, capture:
- thread_id (parent comment id)
- path, line, html_url
- author metadata (login, is_bot)
- ordered list of replies with authors and bodies

If zero threads found, proceed to artifact writing with "no unresolved comments" summary.

</step>

<step name="triage_threads" id="4">

## Triage All Threads

For each thread, classify:
- category: actionable | question | context | nit
- severity: high | medium | low
- reply_worthy: yes/no (heuristic; questions and actionable usually yes)

Keep this pass lightweight — just categorization, no deep analysis.

</step>

<step name="select_targets" id="5">

## Select Threads for Analysis

Default: actionable + question threads.

Adjustments based on intent:
- "just triage" → selection is empty (skip analysis)
- "analyze everything" → include all threads
- User narrowed scope (e.g., "only humans") → honor that

Record the selected thread IDs.

</step>

<step name="spawn_analysis_agents" id="6">

## Deep Analysis with Sub-Agents (1 per Thread)

For each selected thread, spawn a sub-agent using `tools_spawn_agent` with `agent_type=analyzer`.

Each sub-agent receives:
- The single thread: parent comment + all replies
- File path, line number, html_url
- Author information for each comment

Each sub-agent returns:
- Brief problem summary
- Why it matters (risk/impact)
- Proposed resolution path
- reply_draft (if reply_worthy) — do NOT include "🤖" prefix
- target_comment_id to reply to (typically last comment in thread)

Spawn all sub-agents in parallel for efficiency.

</step>

<step name="consolidate_results" id="7">

## Consolidate Sub-Agent Results

Collect all sub-agent responses.

Organize by severity (high → medium → low), then by file path.

For threads not selected for analysis, retain just their triage info.

</step>

<step name="write_artifact" id="8">

## Write Versioned Artifact

Determine artifact filename:
- Call `thoughts_list_active_documents`
- Find existing `pr_{number}_review_comments_*.md` files
- Compute N = 1 + max existing suffix (or 1 if none)
- Filename: `pr_{number}_review_comments_{N}.md`

Artifact content:
- Header: PR number, URL, timestamp
- Triage summary: counts by category and severity
- Analyzed threads section:
  - For each: path:line, url, author(s), category/severity
  - Original thread preserved (all comments with authors)
  - Analysis: problem, impact, resolution
- Non-analyzed threads: brief list (id, path:line, category/severity)
- Footer: Note that reply drafts are in the assistant response

Write using `thoughts_write_document` with doc_type="artifact".

</step>

<step name="present_reply_drafts" id="9">

## Present Reply Drafts in Response

In your assistant response (NOT in the artifact), list reply drafts:

Format each as:
- Thread identifier (path:line or index)
- target_comment_id to reply to
- The draft text (without "🤖" prefix — tool adds it)

Explain: "Say 'send reply for X' to post that reply."

Do NOT call `add_comment_reply` yet — only on explicit user follow-up.

</step>

</process>
