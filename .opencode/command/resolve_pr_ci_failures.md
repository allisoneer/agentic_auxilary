---
description: Triage and remediate PR CI failures; capture evidence and write an artifact
agent: OrchestratorOpenAI
---

<task>
Inspect PR CI for the current PR head SHA, excluding CodeRabbit checks and ignoring only skipped/neutral.

Gather GitHub Actions failure evidence via Bash+gh, attempt only safe grounded remediation, verify any claimed fix, and write a durable artifact.
</task>

<workflow_contract>
1. Follow all steps in order.
2. Use `todowrite` and keep exactly one todo `in_progress`.
3. Create the artifact early and keep updating it with evidence, decisions, actions, and verification.
4. Treat v1 required CI as all visible non-CodeRabbit checks for the current PR head SHA, ignoring only `skipped` and `neutral`.
5. Use Bash+`gh` for raw CI evidence when current Rust/MCP GitHub surfaces are insufficient.
6. Attempt remediation only when the failure cause and the fix are both grounded and bounded.
7. If remediation is unsafe, ambiguous, or inconclusive, stop with explicit handoff details in the artifact.
8. Any claimed code/config fix must be verified with strong checks and at minimum `just check` and `just test` unless impossible.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Establish PR context and todo list

- Determine the current PR number, PR URL, branch, and head SHA.
- Create todos for: context capture, CI evidence collection, analysis, bounded remediation or handoff, verification, artifact finalization.
- Name the artifact with the PR number and head SHA when available.

</step_1>

<step_2>

## Step 2: Capture initial CI state into an artifact

- Create a durable artifact under `thoughts/<branch>/artifacts/` before making changes.
- Record:
  - PR metadata and head SHA
  - invocation context
  - current required-CI policy for this workflow
- Treat the artifact as the running source of truth for all later evidence and decisions.

</step_2>

<step_3>

## Step 3: Gather grounded CI evidence with Bash + gh

In bounded Bash child work, gather best-effort evidence for the current PR head SHA:

- `gh pr view <pr> --json number,url,headRefOid,headRefName,baseRefName,title`
- `gh pr checks <pr> --json name,state,link`
- For failing checks, gather any available GitHub Actions context such as run URLs, job names, and failed log snippets when token scope allows.

Artifact requirements for this step:
- raw or normalized check rows
- required-check interpretation for the current head SHA
- failing check URLs and any mapped run/job URLs
- best-effort failed log snippets or an explicit note that logs were unavailable

</step_3>

<step_4>

## Step 4: Analyze and choose the bounded next action

Classify the situation into one of these buckets:

- `no_failure` — required CI already passed or is still pending
- `safe_fix` — a bounded, grounded remediation is available
- `unsafe_or_ambiguous` — remediation would be speculative, high-blast-radius, or unclear
- `external_rerun_only` — the safest action is a rerun/retry without code changes

When evaluating remediation:
- prefer narrow fixes such as test command/config corrections, missing generated metadata, or obvious mechanical issues
- do not guess at product behavior or make large refactors under CI pressure
- do not claim success until post-fix verification is grounded

</step_4>

<step_5>

## Step 5: Execute safe remediation when justified

If `safe_fix`:
- make only the bounded change required by the grounded CI evidence
- document exactly what changed and why in the artifact
- if a rerun is also necessary, record that explicitly

If `external_rerun_only`:
- perform only the minimum safe rerun action available
- record the exact rerun command/API path used and its outcome

If `unsafe_or_ambiguous`:
- do not force a fix
- write explicit handoff guidance covering likely cause, missing context, and recommended next operator actions

</step_5>

<step_6>

## Step 6: Verify the result before claiming a fix

- Run the strongest appropriate checks for any code/config changes.
- Minimum expected verification when code changed:
  - `just check`
  - `just test`
- Also record any CI-specific follow-up such as whether the relevant GitHub checks were rerun or are still pending.
- If verification fails locally or the remote CI state remains unresolved, update the artifact and fall back to explicit handoff instead of claiming success.

</step_6>

<step_7>

## Step 7: Finalize the artifact and summary

The final artifact must include:
- PR metadata and head SHA
- required-check policy used
- normalized check data
- interpreted required-CI outcome
- failing URLs and best-effort logs
- analysis
- actions taken
- verification results
- explicit handoff details if unresolved

Return a concise summary with:
- artifact path
- whether a fix was made
- verification outcomes
- whether the PR head SHA changed
- what remains if CI is not yet green

</step_7>

</process>

<completion_gate>
Done only when one of these is true:
1. Required CI was analyzed, any safe remediation was completed and verified, and the artifact was finalized.
2. No safe remediation was available, but the artifact contains grounded evidence and explicit handoff instructions.
</completion_gate>
