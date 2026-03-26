---
description: Resume work from a structured OpenAI handoff artifact
agent: NormalOpenAI
---

<task>
Resume interrupted work from a structured handoff artifact. Re-apply the correct workflow instructions by reading the source command file named in the artifact, while carrying forward progress and decisions from the artifact.
</task>

<workflow_contract>
1. Follow all 6 steps in order.
2. Read the handoff artifact fully before any other major action.
3. Re-read the source workflow command file if the artifact names one.
4. Treat the artifact as canonical unless current repository files contradict it.
5. Create `todowrite` immediately after reconstructing state.
6. Do not redo completed work unless the artifact or current files show contradiction or drift.
7. If the artifact contains unresolved blockers that require the user, stop and ask instead of guessing.
8. Treat `confirmed`, `inferred`, and `unconfirmed` validation claims differently; only `confirmed` means verified work.
9. If you create or edit workflow prompt files or `AGENTS.md`, do a post-write comparison pass before treating that work as complete.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Load the Handoff Artifact

1. Parse the handoff artifact path from `<userMessage>`.
2. Treat any remaining text in `<userMessage>` as this session's override instructions.
3. Read the handoff artifact fully before doing anything else.
4. Extract at least these values from the artifact:
   - `source_workflow_command`
   - `source_workflow_file`
   - `resume_mode`
   - `recommended_resume_command` if present
   - anchor documents from `## Resume Protocol`
   - first actions
   - do not redo list
   - remaining work
   - validation status
5. If no valid handoff artifact path is available, ask the user for it and stop.
6. If the artifact is missing important sections, state what is missing and continue only if the remaining information is still usable.

</step_1>

<step_2>

## Step 2: Reconstruct Instructions and Context Separately

1. If `source_workflow_file` is present, read it fully.
2. Read the anchor documents listed under `## Resume Protocol` first.
3. If the artifact also lists `## Files Researched / Wider Context`, read only the items that are still needed for the current resume task.
4. If the artifact references current plan, research, or prior handoff documents, read the relevant ones fully.
5. Treat the source workflow file as the primary instruction set.
6. Treat the handoff artifact as the carried-forward state and progress context.
7. Treat any current-session overrides from `<userMessage>` as higher priority than the artifact.
8. If the artifact's `## Validation Status` includes `inferred` or `unconfirmed` items, treat those as needing fresh verification rather than as proven completion.
9. If the artifact's `## Files Changed` or remaining work includes `.opencode/command/*_openai.md` files or `AGENTS.md`, plan an independent comparison pass before considering prompt-file edits complete.
10. Compare the artifact against the current repo and document state. If a meaningful contradiction exists, report it before continuing.

</step_2>

<step_3>

## Step 3: Rebuild Working State and Report It

1. Summarize:
   - current goal
   - completed work
   - remaining work
   - open questions or risks
   - immediate next action
2. Create a detailed `todowrite` plan.
3. Preserve exactly one `in_progress` item.
4. Carry forward the artifact's `Do Not Redo` items as explicit constraints.
5. If the source workflow is implement_plan-like, break remaining work into granular implementation and verification todos rather than generic phase labels.
6. If the resumed work touches workflow prompt files or `AGENTS.md`, include explicit verification todos for read-back comparison and independent audit.
7. Before major tool work, tell the user:
   - what task you believe you are resuming
   - what is already done
   - what you plan to do next

</step_3>

<step_4>

## Step 4: Resume by Mode

### If `resume_mode` is `continue_source_workflow`

1. Follow the source workflow command file as the primary instruction set.
2. Use the handoff artifact as state and progress context, not as a replacement for the workflow instructions.
3. If `source_workflow_command` is `/implement_plan_openai`, or `/implement_plan` from a legacy pre-migration artifact, or the artifact names requirements plus implementation plan documents, resume from the highest-priority unfinished item and preserve the verify-and-reflect loop before closing a phase.
4. If the plan file's checkmarks conflict with the handoff artifact, report the mismatch and prefer the handoff artifact as the newer progress snapshot unless current files clearly contradict it.
5. If completed work is only `inferred` or `unconfirmed`, schedule verification before treating it as done.

### If `resume_mode` is `continue_generic_work`

1. Use the artifact's remaining work and first actions as the primary task list.
2. Keep the GPT system prompt's verification and completeness rules in force.
3. If the generic work includes workflow prompt files or `AGENTS.md`, require a post-write comparison pass before closing the task.

### If `resume_mode` is `ask_user_first`

1. Present the unresolved blocker clearly.
2. Wait for user guidance instead of guessing.

</step_4>

<step_5>

## Step 5: Continue Carefully

1. Continue with the first unfinished high-priority item.
2. Do not reopen settled decisions unless current files contradict them.
3. Do not retry failed paths unless you have a concrete new reason.
4. If you create or edit workflow prompt files or `AGENTS.md`, run a post-write comparison pass using subagents in parallel; if important ambiguity remains, use `tools_ask_reasoning_model` before declaring completion.
5. Update `todowrite` as you go.

</step_5>

<step_6>

## Step 6: Boundary and Handoff Handling

1. If you reach another natural handoff point or the session grows too large, recommend running `/unwind_openai` again.
2. If the resumed plan changes materially, report the change and update `todowrite` immediately.
3. If a contradiction between the artifact and the current repo emerges mid-work, surface it explicitly before continuing.

</step_6>

</process>

<completion_gate>
You are done only when one of these is true:
1. The resumed work is completed and the user received the result.
2. A real blocker requiring user input was surfaced clearly.
3. You reached a new handoff boundary and recommended `/unwind_openai` for the next transition.
</completion_gate>
