---
description: Begin plan creation with grounded investigation and interactive question resolution (GPT-5.4 optimized)
agent: NormalOpenAI
---

<task>
Begin plan creation (Phase 1). Research and discover relevant code, iterate with the user to resolve open questions, and prepare for finalization. This phase is interactive: present findings, ask targeted questions, and keep refining until the task is ready for Phase 2.
</task>

<workflow_contract>
1. Follow all 5 steps in order.
2. Read every user-provided file or document fully before broader investigation.
3. Summarize your understanding in 2-3 sentences and confirm with the user before deeper investigation.
4. Use `todowrite` in Step 2 and keep exactly one item `in_progress`.
5. Prefer `tools_ask_agent` for discovery and subsystem analysis; use the main agent for orchestration, synthesis, and user interaction.
6. In Step 3, call `tools_ask_reasoning_model` with `prompt_type="reasoning"` to resolve gaps and get recommendations.
7. Do not pretend unresolved questions are settled; ask the user when a decision is required.
8. Do not proceed to final planning until all critical blockers are resolved or explicitly accepted by the user.
</workflow_contract>

**MAKE SURE** you follow ALL 5 steps in order. Track your progress with `todowrite` throughout.

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Gather Task Context

1. Infer the concrete planning target from `<userMessage>`.
2. If the user referenced files, tickets, research documents, or prior artifacts, read each one fully before doing anything else.
3. Summarize your understanding in 2-3 sentences and confirm with the user.
4. If there is still no clear task after reading the supplied context, ask one clarification question and stop.
5. Call `tools_thoughts_list_documents` to see what exists in the current branch. If relevant artifacts are found, such as prior research, existing plans, or prior handoffs, read them fully.
6. Call `tools_thoughts_list_references` to see which reference repositories are available.
7. Decide which references are relevant based on the task context. If uncertain, present the top candidates to the user for selection.

</step_1>

<step_2>

## Step 2: Spawn Parallel Agents

The research document passed into this command contains file:line references and findings. Use these to inform your agent strategy—the research already identified key files and areas.

1. Break the investigation into concrete sub-areas based on the research and current user input.
2. Create a detailed `todowrite` plan for your agent calls. Keep the todos concrete and file- or subsystem-oriented.
3. Launch agents concurrently using `tools_ask_agent`:
   - Spawn 1-3 codebase analyzers for distinct subsystems mentioned in the research.
   - Spawn 1 analyzer per selected reference.
   - Spawn follow-up locators or analyzers if the research identifies tests or integration points that still need mapping.
4. Use the research findings and discovered thoughts artifacts to avoid redoing broad investigation.

**Good example todos**
- `Spawn analyzer for src/services/auth.rs:45-120 to understand token validation flow`
- `Spawn analyzer for the config layer—research mentions RATE_LIMIT_RPS needs to be added`
- `Spawn locator to find tests related to files in research Code References section`

**Bad example todos**
- `Research the codebase`
- `Spawn some agents`

5. If agent results conflict or seem incomplete, spawn follow-up agents to resolve the gaps.

</step_2>

<step_3>

## Step 3: Deep Analysis with the Reasoning Model

Now synthesize what you know versus what remains uncertain. Call `tools_ask_reasoning_model` with `prompt_type="reasoning"` to resolve gaps and get recommendations.

1. Craft a prompt that asks the specific questions you still need answered.
2. Focus the prompt on:
   - resolving open questions and uncertainties
   - getting recommendations on approach and architecture
   - identifying risks, constraints, and edge cases
   - understanding trade-offs between different approaches
3. Include only relevant files and directories with the request rather than passing the entire repo.

<guidance name="file_descriptions">

### File and Directory Descriptions for the Reasoning Model

When passing files and directories to `tools_ask_reasoning_model`, provide concise descriptions that help the optimizer group them intelligently. Describe what the file or directory contains and why it matters for this task.

Good descriptions are 1-2 sentences and mention:
- what this file or directory contains
- why it is relevant to the plan
- any key constraints or interfaces if important

Examples:
- `frontend/src/features/payments/CheckoutForm.tsx` - `Main payment form component that will need new validation logic for subscription upgrades`
- `rust/server/src/services/payments/` - `Payment service layer with strict idempotency requirements; includes PaymentProvider trait`
- `references/allisoneer/payments_integration/README.md` - `Example of similar payment gateway integration with retry/backoff patterns`

Use directories when many related files exist; set `extensions`, `recursive`, and `max_files` appropriately.

</guidance>

</step_3>

<step_4>

## Step 4: Consolidate Findings and Interactive Refinement

After receiving the reasoning model's analysis:

1. Present key findings, design direction, and remaining questions to the user.
2. Include file:line references where helpful.
3. Ask targeted questions to resolve any pending items.
4. Iterate with the user until all critical questions are answered.
5. Update `todowrite` immediately as questions are resolved or new gaps appear.

</step_4>

<step_5>

## Step 5: Close Phase 1 Cleanly

Once all critical questions are resolved:

1. Present a concise summary of the research findings, design direction, and key decisions.
2. Ask if the user needs any clarifications or has remaining questions.
3. When ready, suggest proceeding to Phase 2 with `/create_plan_final_openai`.
4. The finalize phase will write the requirements dossier and generate the full implementation plan.

</step_5>

</process>

<completion_gate>
You are done only when one of these is true:
1. You asked a necessary clarification question because the planning target is still unclear.
2. You presented grounded findings plus targeted open questions and are waiting for the user's answers.
3. Critical questions are resolved, Phase 1 is summarized clearly, all relevant todos are complete, and the user has been told to continue with `/create_plan_final_openai`.
</completion_gate>
