---
description: Drive a Linear ticket from grounded intake through implementation and PR creation (first-pass orchestrator workflow)
agent: OrchestratorOpenAI
---

<task>
Run an end-to-end Linear-ticket-to-PR workflow from the orchestrator layer.

This command should keep routing, scope judgment, and stop/go decisions at the orchestrator level. Child sessions should do bounded work only: read/update Linear state, persist an artifact, research feasibility, plan, implement, verify, commit, generate the PR description, and use bash/gh tooling to push and create the PR.

The goal is to end in one of these grounded states:
- stopped early because no Linear ticket reference was provided
- stopped early because the ticket is not actionable enough and a precise question was posted on Linear
- stopped early because feasibility research surfaced blocking ambiguity and a precise question was posted on Linear
- completed through verified implementation, commit, PR creation, and Linear update with the PR link
</task>

<workflow_contract>
1. Follow all 10 steps in order.
2. Use `todowrite` starting in Step 2 and keep exactly one task `in_progress` at a time.
3. Treat `<userMessage>` as loose natural language. In this first pass, support no modifiers or flags.
4. Require a Linear ticket key, Linear URL, or other identifiable Linear issue reference. If none is present, ask the user for one and stop.
5. Do not create or switch branches. Assume the user already started this command on the correct branch.
6. At every major stage, return control to the orchestrator for the next DAG decision. Child sessions execute bounded work; they do not autonomously continue into later stages.
7. Before downstream codebase work, read the full ticket description and comments, ensure the ticket is `In Progress`, and persist that full corpus to a thoughts artifact.
8. Use a two-layer gate before planning or implementation:
   - first a task-clarity gate
   - then a feasibility-reconnaissance gate when codebase context is still needed
9. If code changes are made, require appropriate verification before claiming completion. At minimum run `just check` and `just test` unless a stronger or more targeted command set is clearly warranted.
10. The commit and PR phase must respect the agent-reset caveat: use `commit`, then re-enter a bash-capable session for the actual git commit/push execution and initial PR creation or update, then use `describe_pr`, then use bash/gh tooling for any final PR update if needed.
11. Linear updates are required when stopping for insufficient scope and when a PR is created. Set the ticket to `In Progress` at the start; at the end, use judgment about any obvious next status, but do not invent statuses.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Resolve the Target Ticket

1. Parse `<userMessage>` as loose natural language.
2. Accept any of these as sufficient ticket references:
   - a Linear issue key such as `ENG-123`
   - a Linear issue URL
   - another identifiable Linear issue reference that a Linear child session can resolve responsibly
3. In this first pass, do not interpret modifiers, flags, or autonomy toggles from the input.
4. If no Linear ticket reference is present, ask the user to provide one and stop.
5. If multiple plausible tickets are referenced and one is not clearly primary, ask one concise clarification question and stop.

</step_1>

<step_2>

## Step 2: Build the Orchestrator Todo List

1. Create concrete todos for:
   - ticket intake and Linear state sync
   - ticket artifact persistence
   - task-clarity gate
   - feasibility reconnaissance if needed
   - research / plan / implementation pipeline
   - verification
   - commit and PR creation
   - final Linear update and summary
2. Keep todos stage-oriented and specific to the resolved ticket.

</step_2>

<step_3>

## Step 3: Read and Normalize Linear Ticket State

1. Spawn a bounded Linear-capable child session.
2. In that child session:
   - resolve the ticket reference
   - read the full issue description
   - read all issue comments, following pagination until complete
   - ensure the ticket status is `In Progress` if it is not already
   - return the canonical ticket identifier, ticket URL, current status, full description text, full comment corpus, and any obviously relevant linked context it could read directly from Linear
3. Do not let the Linear child proceed into codebase research, planning, or implementation.
4. If the ticket cannot be resolved responsibly, return to the user with the specific blocker and stop.

</step_3>

<step_4>

## Step 4: Persist the Ticket Corpus as a Thoughts Artifact

1. Because the orchestrator cannot write artifacts directly and the Linear child may not have thoughts tools, use an explicit handoff pattern:
   - first collect the full ticket description and comments in the Linear child result
   - then spawn a normal or research-capable child session whose bounded task is to write a thoughts artifact containing that full corpus
   - if it is cleaner, fold this into creation of a research-style artifact, but the full ticket description and comments must still be preserved verbatim enough for downstream reuse
2. The artifact should capture at minimum:
   - ticket key and URL
   - current Linear status
   - ticket title
   - full description
   - full comments with authorship and timestamps when available
   - a short note that the artifact is the authoritative ticket corpus for this run
3. Read the produced artifact back in the orchestrator session before proceeding.
4. Do not begin codebase research or planning until this artifact exists.

</step_4>

<step_5>

## Step 5: Run the Task-Clarity Gate

1. From the ticket corpus artifact, decide whether the requested outcome is actionable enough to know what should be built or changed.
2. Look for:
   - a desired outcome
   - success criteria or expected behavior
   - enough scope boundaries to avoid guessing the intended deliverable
3. If the ticket description and comments do not specify an actionable desired outcome well enough:
   - spawn a bounded Linear-capable child session
   - post a precise comment asking only the missing questions needed to make the task actionable
   - return a concise summary to the user and stop
4. If the intended outcome is clear enough to proceed, record that decision and continue.

</step_5>

<step_6>

## Step 6: Run the Feasibility-Reconnaissance Gate

1. Decide whether implementation confidence is already high enough to enter the normal pipeline directly, or whether codebase context is still required first.
2. If feasibility is already clear from existing context, you may skip the reconnaissance session and state why.
3. Otherwise spawn a bounded `research` child session focused specifically on:
   - what parts of the codebase are likely involved
   - whether the requested outcome appears feasible in the current architecture
   - likely risks, constraints, and hidden dependencies
   - what details are still missing
   - what precise questions remain if the ticket is still ambiguous in practice
4. Read the reconnaissance output in the orchestrator session and make the next decision there.
5. If reconnaissance surfaces blocking ambiguity or a missing product/behavior decision:
   - spawn a bounded Linear-capable child session
   - comment on the ticket with the precise questions that emerged
   - stop and report the blocker clearly to the user
6. If reconnaissance indicates the task is understandable and feasible enough to proceed, continue into the standard workflow.

</step_6>

<step_7>

## Step 7: Run the Standard Research and Planning Pipeline

1. Route through the normal pipeline in this order:
   - `research`
   - `create_plan_init`
   - `create_plan_final`
   - `implement_plan`
2. Return to the orchestrator after each stage. Read the produced doc or result before deciding the next stage.
3. Pass the ticket corpus artifact and any reconnaissance artifact/doc paths into downstream stages so later sessions do not need to reconstruct ticket context.
4. If any stage surfaces a blocking ambiguity that must be resolved by the ticket author or team:
   - post the precise question on Linear through a bounded Linear-capable child session
   - stop instead of forcing assumptions
5. Do not skip `create_plan_init` or `create_plan_final` for non-trivial work in this command.

</step_7>

<step_8>

## Step 8: Verify the Implemented Change

1. After `implement_plan`, review the implementation result and verification evidence.
2. Require the strongest appropriate checks for the touched surface area.
3. At minimum, expect `just check` and `just test` unless a stronger or more targeted suite is clearly more appropriate and is actually run.
4. If verification fails, route back through bounded implementation follow-up until the result is either passing or blocked by a real external constraint.
5. Do not proceed to commit or PR creation while verification is still failing.

</step_8>

<step_9>

## Step 9: Commit, Describe, Push, and Create the PR

1. Once implementation is verified, run `commit` to prepare the commit plan.
2. Respect the agent-reset caveat: after `commit`, re-enter a bash-capable child session to perform the actual git commands.
3. In the bash-capable phase:
    - confirm the working tree state
    - create the commit or commits
    - push the branch
    - use `gh` tooling to create the PR if none exists, or update the existing PR if one already exists for the branch
    - the PR must be ready for review, never draft; do not create a draft PR
    - if a created or discovered PR is draft for any reason, run `gh pr ready` before returning control to the orchestrator and verify the PR is no longer draft
4. After the PR exists, run `describe_pr` so the PR description is generated from the actual diff and verification state.
5. If needed, use a follow-up bash-capable child session to apply any final `gh` update steps after `describe_pr` completes.
6. Capture the commit hash or hashes and the final PR URL for the orchestrator summary.

</step_9>

<step_10>

## Step 10: Update Linear and Return the Final Summary

1. Spawn a bounded Linear-capable child session to comment on the ticket with the PR link once the PR is created.
2. If an obvious review/done-adjacent status exists and changing it is clearly appropriate, use judgment; otherwise leave status as-is rather than inventing a workflow state.
3. Return a final summary that includes:
   - ticket key and URL
   - thoughts artifact path for the ticket corpus
   - reconnaissance doc path if one was created
   - research doc path
   - plan doc paths
   - implementation result summary
   - verification commands and outcomes
   - commit hash if available
   - PR URL
   - Linear comment/update status
   - any blockers, caveats, or remaining manual follow-up
4. If the workflow stopped early, say exactly where and why.

</step_10>

</process>

<completion_gate>
You are done only when one of these is true:
1. You stopped immediately because no responsible Linear ticket reference could be determined from `<userMessage>`.
2. You read the ticket, persisted the corpus artifact, determined the task was not actionable enough, posted precise clarification questions on Linear, completed the relevant todos, and returned a grounded stop summary.
3. You read the ticket, persisted the corpus artifact, completed feasibility reconnaissance, found blocking ambiguity, posted precise clarification questions on Linear, completed the relevant todos, and returned a grounded stop summary.
4. You completed the full workflow through research, planning, implementation, verification, commit, push, PR creation, Linear PR-link update, and a final summary containing all required paths and outputs.
5. If code changes were made in this run, do not declare completion unless verification status, commit status, and PR status are all reported explicitly.
</completion_gate>
