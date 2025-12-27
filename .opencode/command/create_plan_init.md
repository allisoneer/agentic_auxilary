---
description: Begin plan creation - research, discover, and iterate with user until ready for finalization
---

<task>
Begin plan creation (Phase 1). Research and discover relevant code, iterate with the user to resolve open questions, and prepare for finalization. This is interactive—you'll present findings and ask questions until ready to proceed.
</task>

**MAKE SURE** you follow ALL 5 steps in the process. Track your progress with todowrite throughout.

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step name="context_gathering" id="1">

## Context Gathering

**User Input:**
- If the user provided parameters (ticket paths, research documents, etc.) in `<userMessage>`, read them fully.
- Summarize your understanding in 2-3 sentences and confirm with the user.
- If no clear task was provided, ask for clarification and wait.

**Thoughts and References:**
1. Call `thoughts_list_active_documents` to see what exists in this branch. If relevant artifacts are found (prior research, existing plans), read them fully.
2. Call `thoughts_list_references` to see available reference repositories.
3. Decide which references are relevant based on the task context. If uncertain, present top candidates to the user for selection.

</step>

<step name="parallel_research" id="2">

## Spawn Parallel Agents

The research document passed into this command contains file:line references and findings. Use these to inform your agent strategy—the research already identified key files and areas.

Use `todowrite` to plan your agent calls. Be specific about what each agent will investigate.

Launch agents concurrently using `tools_spawn_agent`:

- Spawn 1-3 codebase analyzers for distinct subsystems mentioned in the research.
- Spawn 1 analyzer per selected reference.

**Good example todos** (informed by research document):
- "Spawn analyzer for src/services/auth.rs:45-120 to understand token validation flow"
- "Spawn analyzer for the config layer—research mentions RATE_LIMIT_RPS needs to be added"
- "Spawn locator to find tests related to files in research Code References section"

**Bad example todos:**
- "Research the codebase"
- "Spawn some agents"

If agent results conflict or seem incomplete, spawn follow-up agents to resolve gaps.

</step>

<step name="reflect_and_analyze" id="3">

## Deep Analysis with Reasoning Model

Now synthesize what you know versus what remains uncertain. Use `reasoning_model_request` with `prompt_type: "reasoning"` to resolve gaps and get recommendations.

Craft a prompt that asks the questions you need answered. Focus on:
- Resolving open questions and uncertainties
- Getting recommendations on approach and architecture
- Identifying risks, constraints, and edge cases
- Understanding trade-offs between different approaches

Include only relevant files and directories (not exhaustive) with the request. The optimizer will structure the prompt optimally for GPT-5's reasoning mode.

<guidance name="file_descriptions">

### File and Directory Descriptions for the Reasoning Model

When passing files and directories to `reasoning_model_request`, provide concise descriptions that help the optimizer group them intelligently. Describe what the file/directory is and why it matters for this task.

Good descriptions are 1-2 sentences and mention:
- What this file/directory contains
- Why it's relevant to the plan (will be modified, provides context, shows a pattern to follow, etc.)
- Any key constraints or interfaces if important

Examples:
- `frontend/src/features/payments/CheckoutForm.tsx` - "Main payment form component that will need new validation logic for subscription upgrades"
- `rust/server/src/services/payments/` - "Payment service layer with strict idempotency requirements; includes PaymentProvider trait"
- `references/allisoneer/payments_integration/README.md` - "Example of similar payment gateway integration with retry/backoff patterns"

Use directories when many related files exist; set `extensions`, `recursive`, and `max_files` appropriately.

</guidance>

</step>

<step name="consolidate_and_refine" id="4">

## Consolidate Findings and Interactive Refinement

After receiving the reasoning model's analysis:

1. Present key findings, design direction, and remaining questions to the user
2. Include file:line references where helpful
3. Ask targeted questions to resolve any "Pending" items
4. Iterate with the user until all critical questions are answered

</step>

<step name="closure" id="5">

## Closure

Once all critical questions are resolved:

1. Present a concise summary of the research findings, design direction, and key decisions
2. Ask if the user needs any clarifications or has remaining questions
3. When ready, suggest proceeding to Phase 2 with: `/create_plan_final`

The finalize phase will write the requirements dossier and generate the full implementation plan.

</step>

</process>
