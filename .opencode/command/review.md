---
description: Adversarial review of local git changes
agent: ReviewClaude
---

<task>
You are performing adversarial code review on LOCAL git changes, producing original judgments about security, correctness, maintainability, and testing quality.

Constraints:
- Sub-agents have NO git access and NO bash access.
- Diff content is embedded in the reviewer prompt (fileless operation).
- You (main agent) call `review_diff_snapshot` to generate a cached diff, then `review_run` for each lens.

Default output behavior:
- Show only Medium+ severity findings.
- If Low severity findings exist, hide them by default and report "hidden_low_count".
- If the user asks for a more exhaustive/pedantic pass (e.g., "include nits" or "include low severity"), include Low severity findings too.

You MUST follow ALL 6 steps EXACTLY.
</task>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step name="interpret_intent" id="1">

## Interpret Intent (Free-text)

Interpret `$ARGUMENTS` as a natural-language request. Do NOT require or advertise any special syntax.

Resolve these internal parameters (used in later steps):
- mode: default | staged
- paths: [...]
- include_nits: true/false
- focus: "..."

Smart defaults if the user doesn't specify:
- mode=default (review all local changes)
- paths=[] (no path restriction)
- include_nits=false (hide Low severity; still report hidden_low_count)
- focus="" (no extra focus weighting)

Examples of what the user might say (illustrative only, not required formats):
- "review my changes"
- "review just the staged changes"
- "review src/auth.rs and src/db/; focus on error handling"
- "be pedantic and include nits"

How to infer intent:
- Set mode=staged when the user asks for "staged", "index", or "only what's been added"
- Populate paths when the user explicitly names file/dir paths
  - Prefer NOT restricting paths unless the user is explicit; if ambiguous, treat it as focus instead
- Set include_nits=true when the user asks to "include nits", "include low severity", "be exhaustive/pedantic", or similar
- Treat any remaining free-form request as focus (e.g., "focus on security", "focus on edge cases")

Ambiguity policy (match `/review_pr_comments` pattern):
- Only ask a clarifying question if intent is truly ambiguous and a wrong assumption would materially change review scope.
- Otherwise proceed with defaults and disclose assumptions in the final chat response (Step 6).

Record the resolved parameters explicitly (and note any assumptions for Step 6 disclosure).

</step>

<step name="prepare_diff_snapshot" id="2">

## Generate Diff Snapshot (review_diff_snapshot)

Call `review_diff_snapshot` with:
- mode: `"staged"` if mode=staged, else `"default"`
- paths: array of path strings if paths set, else empty array

Example:

```text
review_diff_snapshot(mode="default", paths=[])
review_diff_snapshot(mode="staged", paths=["src/foo.rs", "src/bar.rs"])
```

This returns:
- `diff_handle`: opaque handle for subsequent calls
- `has_changes`: boolean
- `branch_slug`: for artifact naming
- `stats`: files_changed, insertions, deletions
- `paging`: total_pages, total_lines, page_index
- `changed_files`: list of file paths

Store the `diff_handle` and `branch_slug` for use in Steps 3-5.

If `has_changes=false`:
- Continue anyway; write an artifact stating "No changes to review".

If total_lines is very large (e.g., >1500), note in the final artifact that results may be incomplete.

</step>

<step name="spawn_reviewers" id="3">

## Run 4 Lens Reviews (Parallel)

Required lenses (must all succeed for a complete verdict):
- security
- correctness
- maintainability
- testing

Call `review_run` 4 times IN PARALLEL, but RECORD outcome per lens:

### Lens A: Security

```text
review_run(diff_handle=<handle>, lens="security", focus="{focus}")
```

### Lens B: Correctness

```text
review_run(diff_handle=<handle>, lens="correctness", focus="{focus}")
```

### Lens C: Maintainability

```text
review_run(diff_handle=<handle>, lens="maintainability", focus="{focus}")
```

### Lens D: Testing

```text
review_run(diff_handle=<handle>, lens="testing", focus="{focus}")
```

Each call:
- Uses the cached diff snapshot (embedded in prompt; fileless)
- Returns a validated `ReviewReport` with structured findings
- May include `large_diff_warning` if diff exceeds threshold
- Includes `paging` info showing pages reviewed

The `focus` parameter incorporates user-provided focus text as extra weighting.

For each lens, store one execution record:
- On success: `{ lens: "<name>", ok: true, report: <ReviewReport>, large_diff_warning?: <string> }`
- On failure: `{ lens: "<name>", ok: false, error: "<tool error message>" }`

Proceed even if some lenses fail (best-effort); do NOT treat missing lenses as success.

Collect all 4 execution records for consolidation in Step 4.

</step>

<step name="consolidate_results" id="4">

## Consolidate + Deduplicate Findings (file:line)

### Completeness Check (REQUIRED FIRST)

Compute completeness from execution records:
- `succeeded_lenses` = lenses where `ok=true`
- `failed_lenses` = lenses where `ok=false` OR missing from execution records

If `failed_lenses` is non-empty:
- Final status MUST be `incomplete`
- DO NOT output `approved` under any circumstances
- Still consolidate findings from succeeded lenses (partial signal), but clearly label results as incomplete

### Consolidation (only from succeeded lenses)

1) Normalize all succeeded lens outputs into a single list of findings using the shared schema.

2) Group by dedupe key:
- `dedupe_key = "{file}:{line}"` (line is best-effort; if 0, treat as file-level and dedupe by file only)

3) For any group with >1 finding OR conflicting severity/confidence/title:
- Gather context for this dedupe_key:
  - Diff context: call `review_diff_page(diff_handle, page)` to get relevant page content for the file
  - Source context: if `line > 0` and `{file}` exists, read ~20 lines around `{line}` (otherwise skip; treat as file-level)
  - Reminder: `line` values are SOURCE-FILE line numbers; `0` means unknown/unverifiable.
- Call `tools_ask_reasoning_model` with:
  - the grouped candidate findings
  - the gathered diff + source context
  - instruction: output ONE merged finding per dedupe_key
  - rule: prefer highest severity when in doubt; require evidence; keep confidence=medium when uncertain

4) Apply severity filtering:
- Default: include only Medium+.
- If include_nits=true: include Low too.
- Always compute `hidden_low_count`.

### Verdict Computation (with completeness gating)

If `failed_lenses` is non-empty:
- status = `incomplete`

Else (all 4 lenses succeeded):
- status = `needs_changes` if any Critical severity OR (High severity count >= 3)
- otherwise status = `approved`

Sort final findings by severity desc, then file, then line.

</step>

<step name="write_artifact" id="5">

## Write Timestamped Artifact (Final)

Artifact filename:
- `review_<branch_slug>_<timestamp>.md`

Artifact must include:
- Parameters used: mode, paths, include_nits, focus
- Diff summary: files_changed, insertions, deletions, total_pages
- Verdict + counts by severity + hidden_low_count
- **Lens Execution Summary (REQUIRED)**:
  - For each lens (security, correctness, maintainability, testing):
    - If success: findings count + lens verdict + any large_diff_warning
    - If failure: include error message (verbatim, truncate if extremely long >500 chars)
  - If status=`incomplete`: include a note explaining that approval is impossible until all required lenses succeed (recommend rerun)
- Findings list (deduped), each with:
  - category, severity, confidence, file:line, title
  - evidence (quote/snippet from diff)
  - suggested_fix
  - caveat (required if confidence=medium)

Optionally include raw lens outputs under <details> for traceability.

Write via `tools_thoughts_write_document` with:
- doc_type: artifact
- filename: computed above

After writing, call `tools_cli_just_execute` with recipe `thoughts_sync`.

</step>

<step name="present_overview" id="6">

## Present Overview in Chat

If status=`incomplete`:
- Explicitly say: "Verdict: incomplete (failed lenses: <list of failed lens names>)."
- Provide top findings from succeeded lenses, but caveat that review is partial
- Recommend rerunning `/review` to attempt the failed lenses again

If status=`approved` or `needs_changes`:
- Provide a concise overview:
  - Start with a 1–2 line scope summary so the user can confirm what you reviewed:
    - mode (default vs staged), any paths restriction, whether Low severity is included/hidden, and any focus text
    - if you made assumptions in Step 1, disclose them here
  - counts by severity (and note hidden Low count if applicable)
  - top 3 most severe findings with `file:line - title`

Include the artifact filename so the user can reference it.

End with: "What would you like to do next?" (fix issues, focus on a file, rerun asking to include nits/low-severity items, review staged changes only, etc.)

</step>

</process>
