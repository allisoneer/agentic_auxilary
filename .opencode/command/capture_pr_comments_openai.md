---
description: Capture PR review comments and code snapshots into a normalized artifact (GPT-5.4 optimized)
agent: NormalOpenAI
---

<task>
Fetch PR review comments, exhaust the tool's implicit pagination, normalize the results into a conservative thread model, capture bounded code snapshots around anchored comments, and write a versioned artifact for downstream workflows.

This command is intentionally low-judgment. It should document what exists right now, not decide what to fix, what to research, or what to reply.
</task>

<workflow_contract>
1. Follow all 8 steps in order.
2. Default to the current PR, all comment sources, and unresolved comments only unless the user specifies otherwise.
3. Exhaust pagination and gather bounded code snapshots before writing the final artifact.
4. Treat the parent comment ID as the only safe inferred thread identifier unless the tool explicitly exposes something better.
5. If pagination state appears lost or restarted, restart intentionally and document that fact instead of guessing.
6. Use direct bounded `read` windows for anchored code snapshots; do not use sub-agents for this by default.
7. Write a versioned artifact before the final response.
8. Do not call `tools_ask_agent`, `tools_gh_add_comment_reply`, planning workflows, or implementation workflows from this command.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Interpret Intent and Scope

1. Infer scope from `<userMessage>`.
2. Support these common modifiers when the user mentions them:
   - `pr 123` or another explicit PR number
   - `include resolved`
   - `humans only`
   - `robots only`
   - `all comments`
3. If the user does not narrow scope, assume:
   - current PR
   - `comment_source_type=all`
   - `include_resolved=false`
4. Ask a clarifying question only if the PR cannot be determined and the user did not provide enough information to choose one responsibly.
5. Record any assumptions for the final response and artifact header.

</step_1>

<step_2>

## Step 2: Identify the PR Conservatively

1. If the user provided `pr_number`, use it.
2. Otherwise call `tools_gh_get_comments` without `pr_number` and let the tool attempt autodetection.
3. If autodetection fails, call `tools_gh_get_prs`, follow the tool-visible pagination cues, and stop when the tool says completion.
4. If multiple PR candidates remain and you still cannot choose responsibly, ask the user to choose.
5. Treat autodetection as a convenience, not a guarantee.
6. Use GitHub tool output as the authoritative source for PR number, repo, and PR URL.

</step_2>

<step_3>

## Step 3: Exhaust `gh_get_comments` Pagination

1. Call `tools_gh_get_comments` with the chosen PR and scope filters.
2. Repeat the exact same request only while the tool output indicates there are more results.
3. Stop immediately when the tool says the sequence is complete.
4. Do NOT call again after completion, because an identical call restarts from page 1.
5. Be aware that identical calls can also restart after cache expiry; if the sequence appears to rewind unexpectedly before you finished capturing it, restart the capture intentionally from page 1 and note that in the artifact.
6. Preserve each returned page of comments in memory until the full capture is complete.
7. Record:
   - `owner`, `repo`, `pr_number`, `pr_url`
   - `shown_threads`, `total_threads`, `has_more`
   - all returned per-comment fields that are actually present
8. If no matching comments exist, continue through artifact writing with a grounded empty-result artifact.

</step_3>

<step_4>

## Step 4: Normalize Into a Conservative Thread Model

1. Build your own internal thread model from the fetched comments.
2. Treat comments with no `in_reply_to_id` as parent comments.
3. Treat the parent comment's `id` as the inferred thread anchor.
4. Attach replies by `in_reply_to_id`.
5. Preserve comment ordering as returned by the tool whenever possible.
6. For each normalized thread, capture these fields when present:
   - inferred thread anchor = parent `id`
   - `path`, `line`, `side`
   - parent `html_url`
   - ordered comments with `id`, author/login, `is_bot`, `body`, `html_url`, `pull_request_review_id`, `in_reply_to_id`, created/updated timestamps
7. If a field is absent or hidden, mark it unavailable rather than deriving or inventing it.
8. Do not assume a distinct public `thread_id` exists.
9. Note that unresolved-only mode is best-effort rather than perfect if resolution metadata was incomplete upstream.

</step_4>

<step_5>

## Step 5: Capture Bounded Code Snapshots

1. Build a unique set of code anchors from normalized threads where `path` and `line` are both available.
2. Dedupe identical `(path, line)` anchors.
3. Merge obviously overlapping windows in the same file when practical so you do not read the same area repeatedly.
4. Use direct `read` calls on the anchored file paths rather than spawning locator sub-agents.
5. Capture a tight window around each anchor, defaulting to roughly 8 lines before and after the anchored line with line numbers preserved.
6. Label every snippet as a **current checkout snapshot at capture time**, not as a guaranteed historical view of what the reviewer originally saw.
7. If the file is missing, the line is unavailable, or the requested line is out of range, record `snippet unavailable` with the reason.
8. Do not chase moved code, search for broader related logic, or widen the read scope substantially in this command.

</step_5>

<step_6>

## Step 6: Prepare a Versioned Artifact Path

1. Call `tools_thoughts_list_documents` filtered to artifact documents.
2. Look for existing filenames matching `pr_{number}_comments_capture_*.md`.
3. Compute the next version number.
4. Create a filename of the form `pr_{number}_comments_capture_{N}.md`.

</step_6>

<step_7>

## Step 7: Write the Artifact

1. Write the artifact with `tools_thoughts_write_document` using `doc_type="artifact"`.
2. Structure the artifact like this:

### Header
- PR number, PR URL if available, timestamp
- scope assumptions and filters used
- whether autodetection or explicit PR targeting was used
- whether any restart/re-capture occurred

### Capture Notes
- pagination behavior observed
- any warnings about cache expiry, missing fields, or best-effort unresolved filtering
- note whether MCP-visible IDs/URLs appeared incomplete
- code snapshot policy used for this capture

### Summary
- `shown_threads`
- `total_threads`
- comment count
- file paths represented

### Normalized Threads
For each thread:
- inferred thread anchor (parent comment ID)
- `path:line` if available
- parent URL if available
- current code snapshot at capture time, if available
- ordered transcript of parent + replies
- for each comment, print the fields you actually observed

If multiple threads share the same anchor window, reuse the same snippet block or reference it clearly rather than duplicating it unnecessarily.

### Footer
- concise next-step suggestions such as:
  - `run resolve_pr_comments_openai on this PR`
  - `research thread X`
  - `reply manually to thread X`

3. Make the artifact maximally useful as a source document for later automation.
4. Do not add deep technical judgments, reply drafts, or implementation recommendations beyond obvious capture notes.

</step_7>

<step_8>

## Step 8: Return a Compact Capture Summary

1. Summarize what was captured:
   - PR
   - scope used
   - thread count
   - code snapshot coverage
   - artifact path
2. Disclose any important caveats explicitly, including:
   - autodetection fallback use
   - pagination restart
   - missing IDs/URLs
   - snippet unavailable cases
   - best-effort unresolved filtering
3. Do not present triage, fix recommendations, or reply advice beyond telling the user which downstream workflow would consume the artifact.

</step_8>

</process>

<completion_gate>
You are done only when one of these is true:
1. You asked a necessary clarification question because the PR could not be identified responsibly.
2. You exhausted pagination, wrote the artifact, and returned the capture summary with caveats.
</completion_gate>
