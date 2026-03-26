---
description: Autonomously drive PR comment resolution from the orchestrator layer (GPT-5.4 optimized)
agent: OrchestratorOpenAI
---

<task>
Run an end-to-end PR comment workflow from the orchestrator layer.

This command should keep judgment and routing at the orchestrator level. Child sessions should do bounded work only: capture comments, research a disputed cluster, implement an accepted batch, post grounded replies, or refresh state.

The goal is to move every in-scope thread into one of these end states:
- resolved by code change and reply
- replied to with a grounded clarification, decline, defer, or ask-back question
- waiting on reviewer response after an intentional question back in-thread
- explicitly left out of scope with a documented reason
</task>

<workflow_contract>
1. Follow all 8 steps in order.
2. Use `todowrite` and keep exactly one task `in_progress` at a time.
3. Always start from a fresh capture artifact unless the user explicitly approves reusing an existing one.
4. Group related threads before choosing research, planning, implementation, or reply actions.
5. Prefer an in-thread clarifying question when confidence is materially low instead of forcing a shaky decision.
6. Choose the lightest route that still gives high confidence; if information is incomplete, consequences are unclear, or multiple plausible paths remain, escalate to research and/or planned execution rather than forcing a direct change.
7. Any reply that claims a code/doc/config fix must be posted only after the relevant changes are verified, committed, and pushed.
8. If code changes are made outside `implement_plan_openai`, still require strong verification — at minimum `just check` and `just test` unless a stricter command set is clearly necessary.
9. Refresh comment state after material replies or code changes before declaring completion.
10. Do not call `review_pr_comments_openai`; this workflow replaces that session-centric pattern.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Interpret Intent and Establish Autonomy Bounds

1. Infer the requested autonomy level from `<userMessage>`.
2. Support common modifiers such as:
   - `pr 123`
   - `include resolved`
   - `humans only` or `robots only`
   - `no replies yet`
   - `dry run`
   - `capture only`
   - `reply only`
   - `fix only`
3. If the user does not narrow scope, assume:
   - current PR
   - unresolved comments only
   - all comment sources
   - autonomous routing is allowed
4. Ask a clarification question only if you cannot determine the target PR or if the user gave mutually exclusive instructions.
5. Record the explicit limits for this run, especially whether replies and code changes are allowed.

</step_1>

<step_2>

## Step 2: Build the Orchestrator Todo List

1. Create todos for:
   - fresh capture
   - clustering and decision-making
   - each research / implementation / reply batch that emerges
   - final refresh and completion check
2. Keep todos concrete and batch-oriented.
3. Prefer batching related threads together when they share a file, topic, reviewer concern, or explicit cross-reference.

</step_2>

<step_3>

## Step 3: Acquire a Fresh Comment Corpus

1. Spawn `capture_pr_comments_openai` via `orchestrator_run` using the current scope.
2. If the user explicitly provided an existing capture artifact and said to reuse it, you may skip a fresh capture, but only if you call that assumption out clearly.
3. Read the capture artifact after it is produced, including its bounded code snapshots.
4. Treat the capture artifact as the authoritative input corpus and first-pass code context for downstream routing.
5. If capture fails, handle permissions or rerun as needed before continuing.

</step_3>

<step_4>

## Step 4: Cluster Threads and Choose the Smallest Sufficient Route

1. Cluster related threads using signals such as:
   - same file or nearby lines
   - explicit references to another thread
   - same underlying design question
   - reviewer pushback to a previous AI reply
   - multi-part comments that should be split into sub-decisions
2. For each cluster, choose one route:
    - `reply_now` — a grounded clarification/acknowledgement/decline/defer is enough
    - `ask_back` — the safest next move is a clarifying question in-thread
    - `research` — facts or tradeoffs are uncertain and need investigation
    - `bounded_change` — a fully understood low-blast-radius change that can be executed without the full planning pipeline
    - `planned_change` — non-trivial change that should go through plan + implementation workflow
    - `out_of_scope` — explicitly leave untouched with rationale
3. Prefer `ask_back` when:
    - the reviewer comment is multi-part and intent is still unclear
    - an external factual claim is disputed
    - prior AI replies received pushback and confidence is still low
4. Prefer `planned_change` when any of these hold:
   - information is incomplete even after reading the capture artifact and code snapshots
   - blast radius or downstream consequences are not obvious
   - dependency, config-surface, tooling, or cross-crate behavior may change
   - more than one plausible implementation path remains
   - you would be uncomfortable defending the change without a fuller plan and verification loop
5. Use `bounded_change` only when the thread intent, code location, change shape, and verification path are all clear.
6. Do not force a terminal judgment just because a comment exists.

</step_4>

<step_5>

## Step 5: Execute Each Route with Bounded Child Sessions

1. For `reply_now` or `ask_back` clusters:
    - spawn a bounded NormalOpenAI session that reads the relevant capture artifact section,
    - refreshes comment state if needed,
    - drafts and posts the grounded reply/question,
    - reports the exact comment IDs handled.
2. For `research` clusters:
   - run `research_openai` on only the disputed threads or related code/contract area,
   - read the resulting research doc,
   - then reclassify the cluster.
3. For `bounded_change` clusters:
    - spawn a bounded NormalOpenAI session to make the change directly,
    - verify with the strongest appropriate checks and at minimum `just check` plus `just test`,
    - create an atomic commit for the addressed batch,
    - push that commit,
    - only then post grounded replies in a follow-up bounded NormalOpenAI session.
4. For `planned_change` clusters:
    - run `create_plan_init_openai`,
    - then `create_plan_final_openai`,
    - then `implement_plan_openai`,
    - ensure the resulting implementation verification includes `just check` and `just test` or a stronger justified equivalent,
    - create an atomic commit,
    - push it,
    - only then run a bounded NormalOpenAI session to post final replies.
5. For `out_of_scope` clusters:
    - post a reply only if the autonomy bounds allow replies,
    - otherwise record the rationale in your final summary.
6. If multiple code-change clusters are tightly related, you may batch them into one implementation/verification/commit/push sequence before replying, but do not claim a fix for a cluster whose code is not yet pushed.
7. Keep each child prompt narrow. Pass only the cluster-relevant artifact snippets, code snapshots, files, and expectations.
8. Handle permission requests promptly so child sessions do not stall.

</step_5>

<step_6>

## Step 6: Refresh State After Material Progress

1. After any pushed code-change batch and its follow-up replies, run a fresh `capture_pr_comments_openai` pass unless the user asked for dry-run behavior.
2. Read the refreshed artifact and compare it against the prior state.
3. Detect:
   - newly resolved threads
   - reviewer follow-up or pushback
   - still-open threads that need another route
4. If the refreshed state materially changes the routing decision, update the todo list and continue iterating.
5. Do not declare a cluster done based only on what a child session intended to do; confirm with refreshed state when practical.

</step_6>

<step_7>

## Step 7: Decide Whether Another Iteration Is Needed

1. Continue iterating while any in-scope cluster still needs one of:
    - research
    - implementation
    - commit/push
    - reply posting
    - refreshed verification
2. Stop iterating when every in-scope cluster is in one of these grounded states:
    - addressed, verified, committed, pushed, and replied
    - asked back and waiting on reviewer response
    - explicitly deferred or declined with a posted rationale
    - intentionally withheld because the user forbade replies or code changes
3. If the working tree is dirty or local commits are unpushed after code-change work, do not declare completion.
4. If you hit a real ambiguity that the command cannot safely resolve, ask the user a concise question and stop.

</step_7>

<step_8>

## Step 8: Return the Orchestrator Summary

1. Summarize:
    - capture artifacts produced
    - research docs produced
    - plan docs produced
    - implementation batches run
    - commits created
    - push status
    - reply batches run
2. Report final thread states in grouped form:
    - resolved by code + reply
    - replied clarification/decline/defer
    - asked back / awaiting reviewer response
    - intentionally left for user decision
3. Report final repository state:
    - branch name
    - clean vs dirty working tree
    - any remaining modified/untracked files
4. Call out any important caveats such as autodetection fallback, pagination restart, unresolved reviewer follow-up, or intentionally-unpushed work.
5. If the workflow is not fully complete, say exactly what remains and why.

</step_8>

</process>

<completion_gate>
You are done only when one of these is true:
1. You asked a necessary clarification question because the PR or autonomy bounds could not be determined responsibly.
2. You completed at least one full capture-and-route pass with no code changes and returned a grounded orchestrator summary.
3. You iterated until every in-scope thread is either addressed, verified, committed, pushed, and replied to; waiting on reviewer response; or explicitly held for user decision.
4. If code changes were made in this run, the final summary includes commit/push status and final repo state.
</completion_gate>
