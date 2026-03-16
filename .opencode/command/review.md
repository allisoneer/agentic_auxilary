---
description: Adversarial review of local git changes (MVP)
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

Finding schema (for all analysis outputs):
- file: path
- line: number (best-effort new-file line from diff; if unknown use 0 and explain)
- category: security | correctness | maintainability | testing
- severity: critical | high | medium | low
- confidence: high | medium
- title: short
- evidence: quote relevant diff snippet (or describe hunk) + why it indicates the issue
- suggested_fix: concrete next step (code-level when possible)
- caveat: optional, required when confidence=medium
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

Spawn 4 `tools_ask_agent` calls IN PARALLEL, all with:
- agent_type: analyzer
- location: codebase
- Must read `./review.diff` fully first
- Must NOT use git/bash
- Must output findings in the shared schema

### Lens A: Security
Focus on:
- input validation, injection, authn/authz, secrets, crypto misuse
- unsafe deserialization, SSRF, path traversal, command execution
- privilege boundaries, multi-tenant safety, PII leaks

### Lens B: Correctness / Edge Cases
Focus on:
- logic bugs, off-by-one, wrong defaults, error handling
- concurrency/races, timeouts/retries, resource leaks
- API misuse, panic paths, invariants violated by new code

### Lens C: Maintainability / Patterns
Focus on:
- complexity, duplication, unclear naming, missing docs/comments
- architectural drift, layering violations, "quick hacks"
- performance footguns introduced by the diff (only when material)

### Lens D: Testing / Observability
Focus on:
- missing tests for new behavior/edge cases
- regressions likely without coverage
- logging/metrics/tracing gaps, debuggability, failure-mode visibility

Each lens should incorporate `focus` text (if provided) as an extra weighting, without ignoring the lens remit.

Return 4 result blobs labeled clearly with lens name.

</step>

<step name="consolidate_results" id="4">

## Consolidate + Deduplicate Findings (file:line)

1) Normalize all lens outputs into a single list of findings using the shared schema.

2) Group by dedupe key:
- `dedupe_key = "{file}:{line}"` (line is best-effort; if 0, treat as file-level and dedupe by file only)

3) For any group with >1 finding OR conflicting severity/confidence/title:
- Call `tools_ask_reasoning_model` with:
  - the grouped candidate findings
  - instruction: output ONE merged finding per dedupe_key
  - rule: prefer highest severity when in doubt; require evidence; keep confidence=medium when uncertain

4) Apply severity filtering:
- Default: include only Medium+.
- If include_nits=true: include Low too.
- Always compute `hidden_low_count`.

5) Compute verdict (MVP heuristic):
- needs_changes if any Critical severity OR (High severity count >= 3)
- otherwise approved_with_notes

6) Sort final findings by severity desc, then file, then line.

</step>

<step name="write_artifact" id="5">

## Write Timestamped Artifact (Final)

Artifact filename:
- `review_<branch_slug>_<timestamp>.md`

Artifact must include:
- Parameters used: mode, paths, include_nits, focus
- Metadata summary from review.meta.json
- Verdict + counts by severity + hidden_low_count
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

Provide a concise overview:
- counts by severity (and note hidden Low count if applicable)
- top 3 most severe findings with `file:line - title`

Include the artifact filename so the user can reference it.

End with: "What would you like to do next?" (fix issues, focus on a file, rerun with --include-nits, etc.)

</step>

</process>
