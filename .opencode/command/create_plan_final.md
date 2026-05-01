---
description: Finalize plan creation by writing requirements and generating the implementation plan (GPT-5.4 optimized)
agent: NormalOpenAI
---

<task>
Finalize plan creation (Phase 2). Consolidate the approved Phase 1 findings into a requirements dossier, generate a concrete implementation plan, review the result with the user, and persist the final documents.
</task>

<workflow_contract>
1. Follow all 5 steps in order.
2. Treat any additional instructions in `<userMessage>` as requirements for this finalization pass.
3. Use `todowrite` to track dossier writing, gap resolution, plan generation, review, and persistence.
4. Do not generate the final implementation plan while critical questions remain unresolved.
5. Before persisting the final plan, present a concise summary and ask for user feedback.
6. Keep the requirements dossier and implementation plan paired under the same readable base name.
</workflow_contract>

**MAKE SURE** you follow ALL 5 steps in order. Track your progress with `todowrite` throughout.

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Write the Requirements Dossier

1. Read every file, research artifact, prior plan artifact, or other document referenced in `<userMessage>` fully.
2. Treat any extra user instructions in `<userMessage>` as input that may refine or override the Phase 1 outcome.
3. If ANY content appears in `<userMessage>`, take it into account. This is important.
4. Call `tools_thoughts_get_template` with `template=requirements`.
5. Consolidate the Phase 1 findings, user decisions, constraints, and unresolved caveats into a formal requirements dossier.
6. Write the dossier with `tools_thoughts_write_document` using:
   - `doc_type: "plan"`
   - `filename: {readable_descriptive_name}_requirements.md` (use only `A-Za-z0-9._-`, no slashes)
   - `content:` the completed dossier following the requirements template
7. This dossier becomes the formal record of Phase 1 decisions and will be passed directly to the reasoning model in Step 3.
8. Store the returned dossier path for the remaining steps.

</step_1>

<step_2>

## Step 2: Preflight for Remaining Gaps

1. Re-read the requirements dossier you just wrote.
2. Check whether any critical questions are still marked pending, contradictory, or insufficiently grounded.
3. If critical gaps remain, resolve them before plan generation by:
   - asking the user targeted questions, or
   - using `tools_ask_reasoning_model` with `prompt_type="reasoning"`
4. Do not continue to plan generation until the critical blockers are resolved or explicitly accepted by the user.

</step_2>

<step_3>

## Step 3: Generate the Implementation Plan

1. Call `tools_ask_reasoning_model` with `prompt_type="plan"`.
2. Pass:
   - the requirements dossier
   - the key implementation targets
   - any architectural context files or directories
   - any reference examples that materially shape the design
3. For every passed file or directory, describe what it contains and why it matters using the same concise Phase 1 approach in 1-2 sentences.
4. Request a complete, actionable implementation plan with phased execution, concrete file paths, and explicit verification criteria.
5. The reasoning model already has a built-in plan template and optimizer, so pass focused context rather than trying to over-format the request yourself.
6. If the plan output exposes new critical unknowns, return to Step 2 before moving forward.

</step_3>

<step_4>

## Step 4: Format, Review, and Get Approval

1. Call `tools_thoughts_get_template` with `template=plan`.
2. Format the generated implementation plan so it follows the expected template structure.
3. Before persisting, present the user with a compact summary covering:
   - the number of phases and what each phase accomplishes
   - the main technical decisions
   - the testing strategy
4. Ask: `Any feedback or changes before I persist the final plan?`
5. If the user requests changes, incorporate them and repeat this step.

</step_4>

<step_5>

## Step 5: Persist the Final Plan

1. After user approval, write the final plan with `tools_thoughts_write_document` using:
   - `doc_type: "plan"`
   - `filename: {same_readable_name}_implementation.md` (use only `A-Za-z0-9._-`, no slashes)
   - `content:` the approved implementation plan
2. Confirm the saved path of both the requirements dossier and implementation plan.
3. Tell the user the plan is ready for implementation.

</step_5>

</process>

<guidance>

## Handling Mid-Finalization Gaps

If new unknowns emerge during finalization:
1. Ask targeted questions or call `tools_ask_reasoning_model` with `prompt_type="reasoning"` to close the gaps.
2. Update the requirements dossier if the decision changes the scope or constraints.
3. Re-run the plan generation with the corrected context instead of patching around stale assumptions.

## Filename Consistency

Keep requirements and implementation paired under the same base name:
- `{name}_requirements.md`
- `{name}_implementation.md`

Store both returned file paths immediately so you can report them together in the final confirmation.

## Direct Plan Writing Option

You may pass `output_filename` to `tools_ask_reasoning_model` with `prompt_type="plan"` so the plan is written directly into `thoughts/{branch}/plans/`. The tool returns the repo-relative path on success instead of the plan content. If you use this path, still present the review summary before treating the plan as final.

</guidance>

<completion_gate>
You are done only when one of these is true:
1. You are waiting on a necessary user answer to resolve a critical planning gap.
2. You presented the plan summary and are waiting for approval before persistence.
3. The requirements dossier and implementation plan are both saved, all relevant todos are complete, and the user has the final paths plus confirmation that implementation can begin.
</completion_gate>
