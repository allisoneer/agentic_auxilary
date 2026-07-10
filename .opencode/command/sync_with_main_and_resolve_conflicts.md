---
description: Sync current branch with origin/main via merge and resolve bounded mechanical conflicts (no force-push)
agent: Orchestrator
---

<task>
Sync the CURRENT local worktree branch with the latest origin/main using MERGE (not rebase), resolve only bounded mechanical/code-local conflicts, verify, and push normally when safe.

- Local context only (no Linear ticket required).
- Never force-push. Never use rebase in v1.
- If a push is rejected (non-fast-forward) or would require force, STOP with explicit operator instructions.
</task>

<workflow_contract>
1. Follow all steps in order.
2. Use `todowrite` and keep exactly one todo `in_progress`.
3. All git mutation happens via a Bash child session.
4. Conflict auto-resolution is ONLY allowed when ALL eligibility gates pass (see below).
5. Stop immediately (ask a question / hand off) when any explicit stop condition is hit.
6. Verification must run before declaring success.
7. Push is normal `git push` only; never force-push.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Establish context + todo list

- Determine current branch (`git branch --show-current`) and base ref (default `origin/main`, fallback `origin/master` if needed).
- Record whether this was invoked manually or by OuterDAG (treat $ARGUMENTS as optional metadata only).
- Create todos for: preflight, fetch+behind check, merge attempt, conflict resolution (if needed), verification, push, summary.
</step_1>

<step_2>

## Step 2: Preflight safe starting git state (STOP conditions)

In Bash:
- `git status --porcelain=v2`
- `git rev-parse --abbrev-ref HEAD`
- Detect in-progress operations:
  - If rebase in progress: STOP (do not convert to merge automatically).
  - If merge in progress with unmerged paths: continue to conflict workflow (Step 4) rather than starting a new merge.
STOP if:
- working tree is dirty (uncommitted/untracked changes) AND no merge is in progress
- HEAD is detached
- base ref cannot be resolved after fetch
</step_2>

<step_3>

## Step 3: Fetch + merge origin/main (no rebase)

In Bash:
- `git fetch origin --prune`
- If `origin/main` is already merged (ancestor of HEAD), you may still run verification and push if local commits exist.
- Attempt merge:
  - `git merge --no-edit origin/main`
If merge succeeds with no conflicts: proceed to verification (Step 5).
If merge reports conflicts: proceed to Step 4.
</step_3>

<step_4>

## Step 4: Conflict handling (bounded mechanical/code-local only)

### Capture conflicts

In Bash:
- `git diff --name-only --diff-filter=U`
- `git status --porcelain=v1`

### Eligibility gates (ALL must pass or STOP)

STOP if ANY are true:
- Any conflicted file matches a forbidden pattern:
  - lockfiles: `Cargo.lock`, `pnpm-lock.yaml`, `package-lock.json`, `yarn.lock`, `poetry.lock`
  - generated/binary-ish: `*.png`, `*.jpg`, `*.pdf`, `*.svg`, `*.wasm`, `*.bin`
- Conflict kinds include add/add or modify/delete (porcelain codes like `AA`, `DD`, `UD`, `DU`)
- More than 5 conflicted files
- More than 10 total conflict marker blocks across all files (`<<<<<<<` count)
- Any single file has >3 conflict blocks
- Any conflict requires product/semantic choice (unclear intent, competing behaviors, or missing context)

### Mechanical/code-local resolution procedure

- For each eligible conflicted file:
  - Read the file around conflict markers.
  - Resolve by combining both sides when non-overlapping, updating imports/paths/types locally, and keeping changes consistent with nearby code.
  - Do not accept “theirs” or “ours” wholesale unless trivially identical.
  - Write edits, then `git add <file>`.
- After all conflicts resolved:
  - Confirm no unmerged paths remain.
  - Conclude merge with `git commit --no-edit`.

If you hit an out-of-bounds case, STOP and ask the operator to resolve manually, then respond with “ready” to continue.
</step_4>

<step_5>

## Step 5: Verification

Run the strongest appropriate checks. Minimum:
- `just check`
- `just test`
If verification fails: STOP with actionable next steps (do not push).
</step_5>

<step_6>

## Step 6: Push (normal only, never force)

In Bash:
- `git push`
If push is rejected (non-fast-forward) or would require force:
- STOP and instruct operator to resolve remote divergence manually.
- Explicitly state: do NOT use `--force` / `--force-with-lease` in this workflow.
</step_6>

<step_7>

## Step 7: Summary

Report:
- branch + base ref
- old/new HEAD
- whether merge commit was created
- conflicted files (if any) and what was changed
- verification commands + results
- push outcome
</step_7>

</process>

<completion_gate>
Done only when:
- branch is clean
- merge is complete (no unmerged paths)
- verification passed
- normal push succeeded (or was unnecessary because no new commits)
</completion_gate>
