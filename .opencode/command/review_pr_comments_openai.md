---
description: Review PR comments with triage, analysis, and artifact output (GPT-5.4 optimized)
agent: NormalOpenAI
---

<task>
Triage PR review comments, analyze them, and present a structured overview so the user can decide what to do.

Default behavior (no args):
- Fetch all unresolved comments on the current PR
- Triage all threads (`actionable`, `question`, `context`, `nit`) with severity
- Spawn 1 sub-agent per actionable or question thread for deep analysis
- Assess nits inline (`validity`, `effort`, `worth_addressing`) without sub-agents
- Write a versioned artifact with all threads analyzed and nits categorized
- Present an overview summary in the assistant response with categorized comments

The user then decides what to do: fix issues, add TODOs, send replies, investigate further, and so on.
Do NOT proactively draft replies in the user-facing response or suggest sending them — let the user steer.

Keep instructions lean. The system prompt already covers tool mechanics.
</task>

<workflow_contract>
1. Follow all 9 steps in order.
2. Infer intent from `<userMessage>` and use smart defaults when the request is mostly clear.
3. Fetch all relevant comment threads before final triage or conclusions.
4. Use sub-agents for deeper analysis where warranted, but keep nit and context assessment lightweight unless the user asks for more.
5. All threads must be evaluated; the difference is analysis depth, not whether they are analyzed.
6. Write a versioned artifact before giving the final overview.
7. Do not call `tools_gh_add_comment_reply` unless the user explicitly asks for a reply.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Interpret Intent

1. Infer user intent from `<userMessage>`.
2. If the user did not specify scope, default to the current PR, unresolved comments only, and `comment_source_type=all`.
3. Recognize intent modifiers such as:
   - `just triage`
   - `analyze everything`
   - `include resolved`
   - `humans only` or `robots only`
   - `pr 123`
   - `only questions` or `only actionable`
   - `skip nits`
   - `high severity only`
4. Ask a clarifying question only if the requested scope is truly ambiguous.
5. State any default assumptions clearly in the user-facing summary.

</step_1>

<step_2>

## Step 2: Identify the PR

1. Prefer auto-detection by calling `tools_gh_get_comments` without `pr_number` first.
2. If auto-detection fails or the user specified a PR number, use that explicit PR.
3. If the PR is still unknown, call `tools_gh_get_prs`, follow the tool-visible pagination cues if more results remain, stop when the tool says completion, then present the candidates and ask the user to choose.
4. Treat GitHub tool output as the authoritative source for PR identification and PR metadata in this workflow.

</step_2>

<step_3>

## Step 3: Fetch All Relevant Comment Threads

1. Call `tools_gh_get_comments` using the chosen PR and the intent-derived filters.
   - `comment_source_type` from intent (`all` by default)
   - `include_resolved` from intent (`false` by default)
   - `pr_number` from Step 2
2. Handle pagination by repeating the same request only while the tool output says more results remain.
3. Stop immediately when the tool output says the result is complete.
   - Do not call again after completion, because another identical call restarts pagination from page 1.
4. For each thread, capture:
    - thread ID (parent comment ID)
    - file path and line
    - top-level thread URL
    - author login
    - ordered replies with authors and bodies
5. Also capture PR metadata from the tool output:
    - owner/repo
    - PR number
    - PR URL
6. Treat reply URLs, bot markers, dates, and review IDs as optional extras only when the tool output includes them.
7. If a PR URL is not available from GitHub tool output, mark it unavailable or omit it rather than deriving it from another source.
8. If no matching threads exist, continue through artifact writing with a grounded `no unresolved comments` summary.

</step_3>

<step_4>

## Step 4: Triage Every Thread

1. Classify each thread as `actionable`, `question`, `context`, or `nit`.
2. Assign a severity of `high`, `medium`, or `low`.
3. Decide whether the thread is likely `reply_worthy` (`questions` and `actionable` are usually `yes`).
4. Keep this pass lightweight; do not do deep technical analysis yet.

</step_4>

<step_5>

## Step 5: Select Analysis Depth

1. Default deep analysis targets: `actionable` and `question`.
2. Default inline assessment targets: `nit` and `context`.
3. All threads get evaluated — the difference is depth, not whether they are analyzed.
4. Adjust the selection based on user intent, including:
   - skip all analysis for `just triage`
   - use sub-agents for all categories for `analyze everything`
   - omit nits for `skip nits`
   - honor any category or severity narrowing from the user
5. Record which thread IDs will receive deep analysis versus inline assessment.

</step_5>

<step_6>

## Step 6: Analyze Threads at the Right Depth

1. For each deep-analysis thread, spawn a `tools_ask_agent` analyzer with the thread content, file path, line number, URL, and author information.
   - Use `agent_type=analyzer`.
2. Ask each analyzer to return:
   - a concise problem summary
   - why it matters
   - a proposed resolution path
   - a `reply_draft` if the thread is reply-worthy — do NOT include a `🤖` or bot-marker prefix
   - the best target comment ID for a future reply
3. Launch independent thread analyzers in parallel.

</step_6>

<step_6b>

## Step 6b: Inline Assessment for Nits and Context

1. For each `nit` or `context` thread, assess it inline without spawning a sub-agent.
2. For each nit, determine:
   - `validity`: `valid` | `partially-valid` | `invalid`
   - `worth_addressing`: `yes` | `maybe` | `no`
   - `effort`: `trivial` | `small` | `medium`
   - `one_liner`: brief rationale in one sentence
3. Use these heuristics:
   - `Trivial + valid + yes` means a quick win that should usually be fixed
   - `Invalid` usually means decline or ignore
   - `Valid but not worth it` usually means defer to a future PR
   - group related nits by file when that makes the output easier to act on
4. This assessment happens in the main agent context — no extra tool calls are required.

</step_6b>

<step_7>

## Step 7: Consolidate Results

1. Combine the sub-agent results with the inline assessments.
2. Organize findings into these groups:
   - **Actionable** — by severity (`high` → `medium` → `low`), then by file
   - **Context** — informational threads worth surfacing without treating them as action items
   - **Nits: Quick Wins** — `valid` + worth addressing + `trivial` or `small` effort
   - **Nits: Deferred** — valid but not worth addressing in this PR
   - **Nits: Declined** — invalid or not applicable
3. Make sure every included claim is grounded in the actual thread content and any cited code context.
4. This grouping should make it easy to batch-fix quick wins and understand what to skip.

</step_7>

<step_8>

## Step 8: Write a Versioned Artifact

1. Call `tools_thoughts_list_documents` to find existing `pr_{number}_review_comments_*.md` artifacts.
2. Compute the next version number.
3. Write a new artifact with `tools_thoughts_write_document` using `doc_type="artifact"`.
4. Structure the artifact like this:

### Header
- PR number, URL if available from GitHub tool output, timestamp
- scope assumptions
- triage counts by category and severity

### Actionable Threads (Deep Analysis)
For each actionable or question thread:
- `path:line`, `comment_id`, author(s), category, severity
- original comment text in full
- analysis covering problem, impact, and resolution path
- `reply_draft` if any

### Nits: Quick Wins
For each nit worth addressing:
- `path:line`, `comment_id`, author
- original comment text in full (use `<details>` if long)
- assessment: validity, effort, one-line rationale
- suggested fix if obvious

### Context Threads
For each context thread:
- `path:line`, `comment_id`, author
- original comment text in full if it is useful context
- one line explaining why it matters for understanding the PR

### Nits: Deferred
For each valid-but-not-now nit:
- `path:line`, `comment_id`
- one line saying why it is deferred

### Nits: Declined
For each invalid or not-applicable nit:
- `path:line`, `comment_id`
- one line saying why it is declined

### Footer
- quick command reference such as `fix quick wins`, `fix X`, `add TODO for Y`, `reply to Z saying...`, `tell me more about W`

</step_8>

<step_9>

## Step 9: Present the Overview and Hand Control Back to the User

1. Present a compact summary line with counts by category.
   - Example: `Found X comments: Y actionable, Z nits (W quick wins), ...`
2. Show actionable and question threads ordered by severity (`high` → `medium` → `low`).
   - Format each as `` `path:line` — brief summary `` with category tags such as `[actionable/high]` or `[question/medium]`
3. Show any context threads separately using `` `path:line` — why this context matters ``.
4. Show quick wins as `` `path:line` — what to fix ``.
5. Show deferred items as `` `path:line` — why deferred ``.
6. Show declined items as `` `path:line` — why declined ``.
7. Do not surface reply drafts in the response unless the user asks for them.
8. End with `What would you like to do?` and offer user-steerable next actions such as:
   - `fix the quick wins`
   - `fix X and Y`
   - `add TODO for Z` with the appropriate severity
   - `reply to X saying...`
   - `tell me more about X`
   - `create ticket for X` (if Linear is available)
9. Do NOT call `tools_gh_add_comment_reply` unless the user explicitly requests a reply.

</step_9>

</process>

<completion_gate>
You are done only when one of these is true:
1. You asked a necessary clarification question because the PR or scope could not be determined.
2. You produced the versioned artifact and presented the overview summary, and the user can now choose the next action.
</completion_gate>
