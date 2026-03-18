---
description: Adversarial review of local git changes
agent: ReviewClaude
---

<task>
You are performing adversarial code review on LOCAL git changes, producing original judgments about security, correctness, maintainability, and testing quality.

Constraints:
- Sub-agents have NO git access and NO bash access.
- Sub-agents MUST read from ./review.diff (repo root) as their primary input.
- You (main agent) may call `tools_cli_just_execute` to run `just review-prepare` to generate ./review.diff and ./review.meta.json.

Default output behavior:
- Show only Medium+ severity findings.
- If Low severity findings exist, hide them by default and report "hidden_low_count".
- If user passes --include-nits, include Low severity findings.

You MUST follow ALL 6 steps EXACTLY.
</task>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step name="interpret_intent" id="1">

## Interpret Intent & Parse Args

Parse `$ARGUMENTS` for flags:

- `--staged` -> review staged-only changes (index)
- `--files <paths...>` -> restrict review to these pathspecs (space-separated, up to next flag)
- `--include-nits` -> include Low severity findings in output
- `--focus <text>` -> additional focus guidance for reviewers

Smart defaults:
- If no flags provided: mode=default, include_nits=false, paths=[], focus=""
- If user provides free text without `--focus`, treat it as focus text.

Record resolved parameters explicitly:
- mode: default | staged
- paths: [...]
- include_nits: true/false
- focus: "..."

</step>

<step name="prepare_diff_and_metadata" id="2">

## Prepare Diff Snapshot (just review-prepare)

Call `tools_cli_just_execute`:
- Recipe: `review-prepare`
- Args (positional):
  - First arg (mode): `"staged"` if --staged, else `"default"` (or omit for default)
  - Second arg (paths): `"path1 path2 ..."` if paths set, else omit

Examples:
- Default mode, no paths: `just review-prepare`
- Staged mode, no paths: `just review-prepare staged`
- Default mode with paths: `just review-prepare default "src/foo.rs src/bar.rs"`
- Staged mode with paths: `just review-prepare staged "src/foo.rs"`

Then read BOTH files fully:
- `./review.meta.json`
- `./review.diff`

If `has_changes=false` (or diff is empty), continue anyway:
- Still write an artifact stating "No changes to review".

If diff is very large (e.g., >1500 lines), note in the final artifact that results may be incomplete.

</step>

<step name="spawn_reviewers" id="3">

## Spawn 4 Lens Reviewers (Parallel)

Required lenses (must all succeed for a complete verdict):
- security
- correctness
- maintainability
- testing

Spawn 4 `review_spawn` calls IN PARALLEL, but RECORD outcome per lens:

### Lens A: Security
```
review_spawn(lens="security", focus="{focus}")
```

### Lens B: Correctness
```
review_spawn(lens="correctness", focus="{focus}")
```

### Lens C: Maintainability
```
review_spawn(lens="maintainability", focus="{focus}")
```

### Lens D: Testing
```
review_spawn(lens="testing", focus="{focus}")
```

Each call:
- Uses `./review.diff` as the diff source (default)
- Returns a validated `ReviewReport` with structured findings
- May include `large_diff_warning` if diff exceeds 1500 lines

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
  - Diff context: include each candidate's `evidence` AND the surrounding hunk from `./review.diff` for `{file}`
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
- Metadata summary from review.meta.json
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
  - counts by severity (and note hidden Low count if applicable)
  - top 3 most severe findings with `file:line - title`

Include the artifact filename so the user can reference it.

End with: "What would you like to do next?" (fix issues, focus on a file, rerun with --include-nits, etc.)

</step>

</process>
