---
description: Finalize plan creation - write requirements dossier and generate implementation plan
---

<task>
Finalize plan creation (Phase 2). Write a requirements dossier capturing decisions from Phase 1, generate an implementation plan using the reasoning model, review with the user, and persist both documents.
</task>

**MAKE SURE** you follow ALL 5 steps in the process. Track your progress with todowrite throughout.

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step name="write_requirements" id="1">

## Write Requirements Dossier

First, consolidate all the research and decisions from Phase 1 into a formal requirements document.

Call `thoughts_get_template` with `template=requirements` to get the expected format.

if ANY message is in the `<userMessage>` above, take it into account. This is important!

Write the requirements document using `thoughts_write_document`:

**Parameters:**
- `doc_type: "plan"`
- `filename: {readable_descriptive_name}_requirements.md` (use only A-Za-z0-9._-, no slashes)
- `content:` Full dossier following the template structure

This document becomes the formal record of what was decided in Phase 1 and will be passed to the reasoning model for plan generation.

</step>

<step name="preflight" id="2">

## Pre-flight Verification

Before generating the implementation plan, confirm that no critical "Pending" questions remain in the requirements dossier you just wrote. If gaps exist, ask the user or call `reasoning_model_request` with `prompt_type: "reasoning"` to resolve them first.

</step>

<step name="generate_plan" id="3">

## Generate Implementation Plan

Call `reasoning_model_request` with `prompt_type: "plan"` to generate the full implementation plan.

Craft a prompt that describes the feature/task and requests a complete, actionable implementation plan with phased approach, concrete file paths, and automated/manual success criteria.

<guidance name="files_for_reasoner">

### Files to Pass to the Reasoning Model

Include:

1. **The requirements dossier** you just wrote - describe it as containing the consolidated requirements and decisions
2. **All critical implementation targets, architectural context, and reference examples** - use the same concise description approach from Phase 1 (1-2 sentences describing what each file/directory is and why it matters)

The reasoner already has a built-in plan template and optimizer that will structure this optimally.

</guidance>

</step>

<step name="format_and_review" id="4">

## Format and Write Final Plan

Call `thoughts_get_template` with `template=plan` to get the expected format.

Using the plan content from the reasoning model, write the final implementation plan following the template format.

Before writing, present a brief summary to the user:
- Number of phases and what each accomplishes
- Key technical decisions
- Testing strategy

Ask: "Any feedback or changes before I persist the final plan?"

</step>

<step name="persist" id="5">

## Persist Final Plan

On user approval, write the plan using `thoughts_write_document`:

**Parameters:**
- `doc_type: "plan"`
- `filename: {same_readable_name}_implementation.md` (use same base name as requirements file)
- `content:` The approved plan

Confirm the saved path and that the plan is ready for implementation.

</step>

<guidance name="handling_gaps">

## Handling Mid-Finalization Gaps

If new unknowns emerge:
1. Ask targeted questions
2. Optionally call `reasoning_model_request` with `prompt_type: "reasoning"` to close gaps
3. Re-run the "plan" request with updated context

</guidance>

<guidance name="filename_consistency">

## Filename Consistency

Keep requirements and implementation paired with the same base name:
- `{name}_requirements.md`
- `{name}_implementation.md`

Store the returned file paths for future reference.

</guidance>

</process>
