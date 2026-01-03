---
description: Review PR comments — triage, analyze with sub-agents, draft replies
---

<task>
Triage PR review comments and perform targeted deep analysis using sub-agents.

Default behavior (no args):
- Fetch all unresolved comments on the current PR
- Triage all threads (actionable, question, context, nit) with severity
- Spawn 1 sub-agent per actionable/question thread for deep analysis
- Assess nits inline (validity, effort, worth addressing) without sub-agents
- Write a versioned artifact with all threads analyzed and nits categorized as quick-wins/deferred/declined
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
- Actions: triage all threads; deep-analyze actionable/question; inline-assess nits
- Output: versioned artifact (with nits categorized) + reply drafts in response

Recognize common intents:
- "just triage" → skip all analysis (deep and inline), just categorize
- "analyze everything" → use sub-agents for ALL categories (including nits)
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

**Deep analysis (sub-agents):** actionable + question threads.
**Inline assessment (no sub-agent):** nit + context threads.

All threads get evaluated — the difference is depth, not whether they're analyzed.

Adjustments based on intent:
- "just triage" → skip all analysis (deep and inline)
- "analyze everything" → use sub-agents for ALL categories
- "skip nits" → exclude nit category entirely
- User narrowed scope (e.g., "only humans") → honor that

Record selected thread IDs by analysis type.

</step>

<step name="spawn_analysis_agents" id="6">

## Deep Analysis with Sub-Agents (1 per Thread)

For each **actionable/question** thread, spawn a sub-agent using `tools_spawn_agent` with `agent_type=analyzer`.

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

<step name="assess_nits_inline" id="6b">

## Inline Assessment for Nits/Context (No Sub-Agent)

For each **nit/context** thread, perform a quick inline assessment WITHOUT spawning a sub-agent. This is cheaper but still provides actionable information.

For each nit, determine:
- **validity**: valid | partially-valid | invalid (is the suggestion technically correct?)
- **worth_addressing**: yes | maybe | no (cost/benefit for this PR)
- **effort**: trivial | small | medium (how hard to fix?)
- **one_liner**: Brief rationale (1 sentence max)

Guidelines:
- "Trivial + valid + yes" = quick win, should fix
- "Invalid" = politely decline or ignore
- "Valid but not worth it" = acknowledge, defer to future PR
- Group by file when multiple nits target the same file

This assessment happens in the main agent's context — no tool calls needed, just reasoning over the comment text.

</step>

<step name="consolidate_results" id="7">

## Consolidate All Results

Collect:
1. Sub-agent responses (deep analysis for actionable/question)
2. Inline assessments (nits/context)

Organize into sections:
- **Actionable** — by severity (high → medium → low), then by file
- **Nits: Quick Wins** — valid + worth addressing + trivial/small effort
- **Nits: Deferred** — valid but not worth addressing in this PR
- **Nits: Declined** — invalid or not applicable

This grouping makes it easy to batch-fix quick wins and know what to skip.

</step>

<step name="write_artifact" id="8">

## Write Versioned Artifact

Determine artifact filename:
- Call `thoughts_list_active_documents`
- Find existing `pr_{number}_review_comments_*.md` files
- Compute N = 1 + max existing suffix (or 1 if none)
- Filename: `pr_{number}_review_comments_{N}.md`

Artifact content:

### Header
- PR number, URL, timestamp
- Triage summary: counts by category and severity

### Actionable Threads (Deep Analysis)
For each actionable/question thread:
- path:line, comment_id, author(s), category/severity
- Original comment (preserve full text)
- Analysis: problem, impact, resolution
- Reply draft (if any)

### Nits: Quick Wins
For each nit worth addressing:
- path:line, comment_id, author
- Original comment (full text, use `<details>` if long)
- Assessment: validity, effort, one-liner rationale
- Suggested fix (if obvious)

### Nits: Deferred
For each valid-but-not-now nit:
- path:line, comment_id
- One-liner: why deferred

### Nits: Declined
For each invalid/not-applicable nit:
- path:line, comment_id
- One-liner: why declined

### Footer
- Note that reply drafts are in the assistant response
- Quick command reference: "send reply for X", "fix quick wins", etc.

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
