---
description: Implement an approved technical plan phase by phase
---

# Implement Plan

<task>
Implement an approved technical plan phase by phase. For each phase: make the changes, verify they work, then move to the next phase. Track every change and verification as a todo item so nothing gets missed.
</task>

**MAKE SURE** you follow ALL 4 steps in the process. Step 1 loads context and creates your detailed todo list. Steps 2-4 form the implementation loop: implement phase → verify and reflect → iterate (loop back or complete). Track your progress with todowrite throughout—every change and every verification should be a todo item.

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step name="load_plan_context" id="1">

## Load Plan and Context

**Validate Input:**
- If no plan path provided in `<userMessage>`, return an error asking the user to provide the path to an implementation plan.
- Read the plan fully. If it doesn't exist, return an error.

**Load Related Context:**
1. If the plan filename ends with `_implementation.md`, read the sibling `{pair_base}_requirements.md`. Warn if not found but continue.
2. Call `thoughts_list_active_documents` filtered to `doc_type == "artifact"` with filename starting `plan_{basename}_`. Read any artifacts to understand prior progress.
3. Read all files referenced in the plan.

**Create Implementation Todo List:**

Parse the plan's phases and create granular todos. For each phase, create:
- One todo per significant change (not just "Phase 1" but each file/component change)
- One verification todo for that phase's success criteria

**Good example todos:**
- "Phase 1: Add rate_limit.rs middleware file" — New file with RateLimiter struct and middleware fn
- "Phase 1: Update router.rs to use rate limiter" — Import and wrap routes
- "Phase 1: Add RATE_LIMIT_RPS config key" — Add to config.rs with default 100
- "Verify Phase 1" — Run `cargo test rate_limit`, check middleware applies to /api routes
- "Phase 2: Add Redis backend for rate limiting" — Add redis client, update RateLimiter

**Bad example todos:**
- "Phase 1"
- "Do the implementation"
- "Run tests"

</step>

<step name="implement_phase" id="2">

## Implement Current Phase

For the current phase's implementation todos:

1. **Mark the first incomplete implementation todo as in_progress**
2. **Execute the change** exactly as specified in the plan
3. **Handle divergence** — If codebase differs from plan expectations:
   - STOP and capture: Expected vs Found, Why it matters
   - Optionally call `reasoning_model_request` with `prompt_type: "reasoning"` to evaluate adaptation paths
   - Present options and wait for user guidance before proceeding
4. **Mark the implementation todo complete**
5. **Repeat** for each implementation todo in this phase

Do NOT run verification yet—that's step 3.

</step>

<step name="verify_and_reflect" id="3">

## Verify and Reflect

**Run Verification:**
1. Mark the phase's verification todo as in_progress
2. Run ALL verification commands from the plan's success criteria for this phase
3. Check for regressions (run broader test suite if specified)

**Reflect — Did I Complete Everything?**

Before marking verification complete, explicitly check:
- Did I complete ALL implementation todos for this phase?
- Did ALL verification commands pass?
- Are there any gaps, skipped items, or partial implementations?

**Identify Gaps:**
If anything was missed or failed:
- Note specifically what's incomplete
- Do NOT mark verification todo complete
- These gaps will be addressed in step 4

If everything passed:
- Mark verification todo complete
- Proceed to step 4

</step>

<step name="iterate" id="4">

## Iterate

Check your todo list and decide what to do next:

**If current phase has gaps:**
- Return to Step 2 (Implement Current Phase) to address the gaps
- Focus only on the incomplete items

**If current phase is complete but more phases remain:**
- Return to Step 2 (Implement Current Phase) with the next phase
- Start with the first implementation todo of that phase

**If all phases are complete:**
- Run final verification across all success criteria
- Summarize status: completed, any issues encountered, any divergences handled
- Report completion to user

</step>

<guidance name="resume_protocol">

## Resume Protocol

If returning to partially complete plan:
- Trust checkmarks indicate completed work
- Review context artifacts (plan_{basename}_*) for journey understanding
- Start from first unchecked item

</guidance>

<guidance name="mismatch_handling">

## Mismatch Handling

When encountering significant divergence:
- Present clear format: Expected, Found, Why this matters, How should I proceed?
- Wait for guidance before proceeding if plan explicitly requires it
- Use reasoning model for complex architectural decisions if needed

</guidance>

</process>
