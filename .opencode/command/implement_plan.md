---
description: Implement an approved technical plan phase by phase (GPT-5.4 optimized)
agent: NormalOpenAI
---

<task>
Implement an approved technical plan phase by phase. For each phase, make the planned changes, verify them, reflect on gaps, and iterate until the plan is complete or a real mismatch requires user input.
</task>

<workflow_contract>
1. Follow all 4 steps in order.
2. Read the implementation plan fully before doing any code changes.
3. Use `todowrite` in Step 1 to create granular implementation and verification todos.
4. Keep exactly one todo `in_progress` at a time and mark items complete immediately after finishing them.
5. Do not skip verification; each phase must be implemented, verified, and reflected on before advancing.
6. If the codebase materially diverges from plan expectations, stop, document the mismatch, and ask the user before proceeding.
7. When resuming partial work, trust completed checkmarks and prior artifacts unless current files clearly contradict them.
</workflow_contract>

**MAKE SURE** you follow ALL 4 steps in order. Step 1 loads context and creates your detailed todo list. Steps 2-4 form the implementation loop: implement phase → verify and reflect → iterate.

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Load Plan Context and Build the Todo List

1. Parse the implementation plan path from `<userMessage>`.
2. If no plan path was provided, ask the user for the implementation plan path and stop.
3. Read the implementation plan fully. If it does not exist, report that clearly and stop.
4. If the plan filename ends with `_implementation.md`, read the sibling `{pair_base}_requirements.md`. Warn if it is missing, but continue.
5. Call `tools_thoughts_list_documents` filtered to artifact documents and look for prior handoff or progress artifacts whose filenames start with `plan_{basename}_`.
6. Read any matching artifacts so you understand completed work, past decisions, and resume context.
7. Read all files explicitly referenced in the implementation plan before editing them.
8. Parse the plan into granular todos. For each phase, create:
   - one todo per significant implementation change
   - one verification todo for that phase's success criteria
9. If the plan already contains completed checkmarks or the artifacts show prior completion, reflect that in the initial todo state unless the current code clearly contradicts it.

**Good example todos**
- `Phase 1: Add rate_limit.rs middleware file` — New file with RateLimiter struct and middleware fn
- `Phase 1: Update router.rs to use rate limiter` — Import and wrap routes
- `Phase 1: Add RATE_LIMIT_RPS config key` — Add to config.rs with default 100
- `Verify Phase 1` — Run `cargo test rate_limit`, check middleware applies to `/api` routes
- `Phase 2: Add Redis backend for rate limiting` — Add redis client, update RateLimiter

**Bad example todos**
- `Phase 1`
- `Do the implementation`
- `Run tests`

</step_1>

<step_2>

## Step 2: Implement the Current Phase

1. Identify the first incomplete implementation todo in the current phase and mark it `in_progress`.
2. Make the planned change exactly where the plan says it belongs.
3. Before every edit, read the target file fully enough to understand the existing implementation.
4. If the codebase differs meaningfully from the plan, stop and capture:
   - Expected
   - Found
   - Why it matters
   - Plausible adaptation paths, if known
5. For complex mismatches, you may call `tools_ask_reasoning_model` with `prompt_type="reasoning"` to evaluate options.
6. Present the options and wait for user guidance before proceeding.
7. Mark the implementation todo complete immediately after finishing it.
8. Repeat until all implementation todos for the current phase are complete.

Do NOT run verification yet—that is Step 3.

</step_2>

<step_3>

## Step 3: Verify and Reflect

**Run Verification**

1. Mark the current phase's verification todo `in_progress`.
2. Discover relevant verification commands with `tools_cli_just_search`.
3. Execute the necessary verification recipes with `tools_cli_just_execute` (for example `check` or `test`).
4. Run all applicable verification commands for the current phase using `tools_cli_just_search` and `tools_cli_just_execute`, plus any broader regression checks.

**Reflect — Did I Complete Everything?**

5. Before marking verification complete, explicitly confirm:
   - all implementation todos for the phase are complete
   - all required verification commands passed
   - no gaps, skipped items, or partial implementations remain

**Identify Gaps**

6. If verification fails or a gap remains, do not mark the verification todo complete.
7. If everything passed, mark the verification todo complete.

</step_3>

<step_4>

## Step 4: Iterate or Finish

1. Check the todo list after reflection.
2. If the current phase still has incomplete work, return to Step 2 and finish the gaps.
3. If the current phase is complete and more phases remain, return to Step 2 with the next phase.
4. If all phases are complete, run final verification across the full success criteria.
5. Summarize completed work, verification results, any issues encountered, and any divergences handled.

</step_4>

</process>

<guidance>

## Resume Protocol

When returning to partially completed implementation work:
1. Trust completed checkmarks in the plan and prior `plan_{basename}_*` artifacts as the newest progress record unless current files clearly contradict them.
2. Start from the first unchecked or incomplete item.
3. Preserve the implement → verify → reflect loop instead of jumping straight to later phases.

## Mismatch Handling

When encountering significant divergence from the plan:
1. Present the mismatch using the format: Expected, Found, Why this matters, How should I proceed?
2. Wait for user guidance before proceeding.
3. Use the reasoning model only to clarify options, not to silently rewrite the plan.

</guidance>

<completion_gate>
You are done only when one of these is true:
1. You are waiting on the user because no plan path was provided or a material mismatch requires guidance.
2. The implementation is complete, all relevant verification passed, all todos are complete, and the user received a grounded summary of changes and verification.
</completion_gate>
