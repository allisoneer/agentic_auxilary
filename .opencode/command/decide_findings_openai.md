---
description: Autonomously drive findings resolution from the orchestrator layer (GPT-5.4 optimized)
agent: OrchestratorOpenAI
---

<task>
Run an end-to-end findings resolution workflow from the orchestrator layer.

This command keeps judgment and routing at the orchestrator level. Child sessions do bounded work only: research disputed areas, apply bounded cleanups, execute planning pipelines, or verify state.

The goal is to move every in-scope finding into one of these terminal states:
- resolved by bounded cleanup (verified, committed, pushed)
- resolved via plan + implementation (verified, committed, pushed)
- accepted as context (no action needed, rationale documented)
- explicitly deferred with rationale
- explicitly declined with rationale
- research-pending (investigation spawned, awaiting reclassification)
</task>

<workflow_contract>
1. Follow all 9 steps in order.
2. Use `todowrite` and keep exactly one task `in_progress` at a time.
3. Always normalize findings into an internal corpus before routing.
4. Cluster related findings before choosing routes.
5. Prefer `research` when evidence is weak, disputed, or blast radius is unclear.
6. Prefer `plan_and_implement` when multi-file changes, design decisions, or test harness work are involved.
7. Any cleanup claiming a fix must happen only after verification (`just check` + `just test`), commit, and push.
8. Update the source artifact with a backlink to the decision document after writing it.
9. Refresh findings state after material progress before declaring completion.
10. Do not use session-centric findings commands; this workflow replaces that pattern with orchestrator-driven resolution.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Interpret Intent and Establish Autonomy Bounds

1. Extract the source artifact path from `<userMessage>`. If no path is provided, ask for one and stop.
2. Infer the requested autonomy level from `<userMessage>`.
3. Support common modifiers such as:
   - `cleanup only` — only execute `cleanup_now` routes, defer everything else
   - `planning focus` — bias toward identifying work that needs planning rather than quick cleanup
   - `research first` — spawn research for any finding with confidence gaps before routing
   - `dry run` — route and document decisions but do not execute changes
   - `high severity only` — process only high-severity findings
   - `include low` — include low-severity findings (normally skipped)
   - category filters like `only testing`, `only security`, etc.
4. If the user does not narrow scope, assume:
   - all Medium+ severity findings are in scope
   - autonomous routing is allowed
   - code changes and commits are allowed
5. Ask a clarification question only if you cannot determine the source artifact or if the user gave mutually exclusive instructions.
6. Record the explicit limits for this run, especially whether code changes and commits are allowed.

</step_1>

<step_2>

## Step 2: Build the Orchestrator Todo List

1. Create todos for:
   - read and classify source artifact
   - normalize findings into internal corpus
   - clustering and route assignment
   - each research / cleanup / planning batch that emerges
   - refresh and iteration check
   - final decision artifact and backlink update
2. Keep todos concrete and batch-oriented.
3. Prefer batching related findings together when they share a file, component, or remediation path.

</step_2>

<step_3>

## Step 3: Read and Normalize the Findings Corpus

1. Read the source artifact fully.
2. Classify the source type:
   - **Review artifact**: contains numbered finding blocks with severity, file/line, category, confidence, evidence, and suggested fix fields.
   - **Research document**: contains findings in `## Summary of Findings` or `## Detailed Findings` sections with file:line references but less rigid structure.
3. Extract each discrete finding and normalize into this internal schema:
   - `id`: sequential number
   - `summary`: one-line description
   - `severity`: high | medium | low (from source or inferred)
   - `source_ref`: location in source document (e.g., `Finding 3` or `line 45-52`)
   - `file_line`: primary file:line reference
   - `category`: e.g., correctness, testing, naming, performance, security
   - `evidence`: key supporting facts from source
   - `suggested_fix`: if available from source
   - `status`: `pending` (initial state for all)
4. If the document does not contain discrete findings or evidence, ask a clarification question and stop.
5. Note the source basename and timestamp for artifact naming.
6. Treat this normalized corpus as the authoritative input for downstream routing.

</step_3>

<step_4>

## Step 4: Cluster Findings and Choose Routes

1. Cluster related findings using signals such as:
   - same file or nearby lines
   - same component or module
   - shared remediation path (e.g., all need the same test harness)
   - explicit cross-references between findings
   - findings that should be split into sub-decisions
2. For each cluster, choose one route:
   - `cleanup_now` — bounded, single-file fix with clear scope and high confidence
   - `research` — evidence is weak, disputed, or blast radius is unclear; needs investigation
   - `plan_and_implement` — multi-file changes, design decisions, or test harness work required
   - `accept` — valid context/constraint, no action needed
   - `defer` — valid but out of scope or lower priority for current work
   - `decline` — invalid premise, already addressed, or not applicable
3. Prefer `research` when:
   - the finding's evidence is disputed or incomplete
   - blast radius or downstream consequences are unclear
   - you would be uncomfortable making a decision without more investigation
4. Prefer `plan_and_implement` when any of these hold:
   - the fix spans multiple files
   - design decisions or API changes are involved
   - new test infrastructure or harness work is needed
   - more than one plausible implementation path remains
5. Use `cleanup_now` only when the finding's validity, fix location, change shape, and verification path are all clear.
6. Do not force a terminal judgment just because a finding exists; use `research` to gather confidence first.

</step_4>

<step_5>

## Step 5: Execute Each Route with Bounded Child Sessions

1. For `cleanup_now` clusters:
   - Spawn a bounded NormalOpenAI session that:
     - Reads the relevant file(s) and finding context
     - Applies the fix
     - Verifies with `just check` and `just test`
     - Creates an atomic commit for the addressed findings
     - Pushes that commit
     - Reports the exact finding IDs resolved
   - Mark findings as `resolved_cleanup` after push confirmation.

2. For `research` clusters:
   - Run `research_openai` scoped to only the disputed findings or related code area.
   - Read the resulting research document.
   - Reclassify the cluster based on new evidence.
   - If research resolves ambiguity, assign a new route and continue.
   - If research introduces new questions, document them and either iterate or mark as `research_pending`.

3. For `plan_and_implement` clusters:
   - Run `create_plan_init_openai` with the cluster's findings as input.
   - Run `create_plan_final_openai` to complete the plan.
   - Run `implement_plan_openai` to execute the plan.
   - Ensure implementation verification includes `just check` and `just test`.
   - Create an atomic commit.
   - Push it.
   - Mark findings as `resolved_planned` after push confirmation.

4. For `accept` clusters:
   - Record the rationale documenting why no action is needed.
   - Mark findings as `accepted`.

5. For `defer` clusters:
   - Record the rationale and optionally note when to revisit.
   - Mark findings as `deferred`.

6. For `decline` clusters:
   - Record the rationale with evidence for why the finding is invalid or not applicable.
   - Mark findings as `declined`.

7. If multiple code-change clusters are tightly related, you may batch them into one implementation/verification/commit/push sequence, but do not claim resolution for a cluster whose code is not yet pushed.

8. Keep each child prompt narrow. Pass only the cluster-relevant finding records, file snippets, and expectations.

9. Handle permission requests promptly so child sessions do not stall.

</step_5>

<step_6>

## Step 6: Refresh State After Material Progress

1. After any pushed code-change batch, re-verify the findings corpus state:
   - Which findings are now resolved?
   - Did any changes invalidate other pending findings?
   - Are there any new issues introduced?
2. Update the internal corpus with current status for each finding.
3. If you spawned research that returned new evidence:
   - Re-read the research document.
   - Reclassify affected findings with the new information.
4. Do not declare a finding resolved based only on what a child session intended to do; confirm with actual verification results.

</step_6>

<step_7>

## Step 7: Decide Whether Another Iteration Is Needed

1. Continue iterating while any in-scope finding still needs one of:
   - research
   - implementation
   - commit/push
   - verification
2. Stop iterating when every in-scope finding is in one of these terminal states:
   - `resolved_cleanup` — fix applied, verified, committed, pushed
   - `resolved_planned` — plan executed, verified, committed, pushed
   - `accepted` — context only, no action needed
   - `deferred` — explicitly postponed with rationale
   - `declined` — not applicable with rationale
   - `research_pending` — investigation spawned, awaiting external input
3. If the working tree is dirty or local commits are unpushed after code-change work, do not declare completion.
4. If you hit a real ambiguity that the orchestrator cannot safely resolve, ask the user a concise question and stop.

</step_7>

<step_8>

## Step 8: Write the Decision Artifact and Update Source Backlink

### Write the Decision Artifact

1. Use `tools_thoughts_write_document` with:
   - `doc_type: "artifact"`
   - `filename: findings_decision_{source_basename}_{timestamp}.md`

2. Structure the artifact as follows:

```markdown
# Findings Decision: {source_basename}

## Parameters
| Field | Value |
|-------|-------|
| Source Document | `{source_path}` |
| Source Type | {research | review} |
| Timestamp | {ISO timestamp} |
| Scope Modifiers | {modifiers or "none"} |
| Autonomy Bounds | {what was allowed: changes, commits, research, etc.} |

## Execution Summary
| Metric | Count |
|--------|-------|
| Total findings processed | X |
| Research sessions spawned | X |
| Plans created | X |
| Cleanup batches executed | X |
| Commits created | X |

## Decision Summary
| State | Count | High | Medium | Low |
|-------|-------|------|--------|-----|
| resolved_cleanup | X | ... | ... | ... |
| resolved_planned | X | ... | ... | ... |
| accepted | X | ... | ... | ... |
| deferred | X | ... | ... | ... |
| declined | X | ... | ... | ... |
| research_pending | X | ... | ... | ... |

## Resolved by Cleanup
For each finding:
### {id}. {summary} [{severity}]
- **Source Ref**: {source_ref}
- **File**: `{file_line}`
- **Category**: {category}
- **Fix Applied**: {description of change}
- **Verification**: {just check + just test results}
- **Commit**: {commit hash}

## Resolved by Planning
For each finding:
### {id}. {summary} [{severity}]
- **Source Ref**: {source_ref}
- **File(s)**: `{file_line}` (and related)
- **Category**: {category}
- **Plan Document**: `{plan_path}`
- **Implementation Summary**: {what was done}
- **Verification**: {just check + just test results}
- **Commit**: {commit hash}

## Accepted
For each finding:
- **{id}. {summary}** [{severity}] — {rationale for accepting as context}

## Deferred
For each finding:
- **{id}. {summary}** [{severity}] — {rationale and when to revisit}

## Declined
For each finding:
- **{id}. {summary}** [{severity}] — {rationale with evidence}

## Research Pending
For each finding:
- **{id}. {summary}** [{severity}] — {research spawned and what question remains}

## Repository State
| Field | Value |
|-------|-------|
| Branch | {branch name} |
| Working Tree | {clean | dirty} |
| Unpushed Commits | {count or "none"} |
| Modified Files | {list or "none"} |
```

### Update the Source Artifact with Backlink

1. Read the source artifact.
2. Check if a `## Linked Decision Documents` section exists at the end.
3. If it does not exist, append it.
4. Add or update an entry for this decision document:

```markdown
## Linked Decision Documents

| Document | Created | Status | Findings Resolved |
|----------|---------|--------|-------------------|
| `{decision_artifact_path}` | {timestamp} | {Complete | In Progress} | {X of Y} |
```

5. If the source artifact has a `github_url` available, include it in the link.
6. Write the updated source artifact back.

</step_8>

<step_9>

## Step 9: Return the Orchestrator Summary

1. Summarize:
   - source artifact processed
   - research documents produced
   - plan documents produced
   - cleanup batches executed
   - commits created
   - push status
2. Report final finding states in grouped form:
   - resolved by cleanup
   - resolved by planning
   - accepted as context
   - deferred
   - declined
   - research pending
3. Report final repository state:
   - branch name
   - clean vs dirty working tree
   - any remaining modified/untracked files
4. Confirm the backlink was added to the source artifact.
5. Report the decision artifact path.
6. Call out any important caveats such as research still pending, findings that need user input, or intentionally-skipped low-severity items.
7. If the workflow is not fully complete, say exactly what remains and why.

</step_9>

</process>

<completion_gate>
You are done only when one of these is true:
1. You asked a necessary clarification question because the source artifact path was missing, the document lacks discrete findings, or findings are not decisionable.
2. You completed at least one full normalize-and-route pass with no code changes (dry run) and returned a grounded orchestrator summary.
3. You iterated until every in-scope finding is in a terminal state: resolved (cleanup or planned), accepted, deferred, declined, or research-pending with investigation spawned.
4. If code changes were made in this run, the final summary includes commit/push status, final repo state, and confirmation that the source artifact backlink was updated.
5. The decision artifact has been written and the source artifact has been updated with a backlink to it.
</completion_gate>
