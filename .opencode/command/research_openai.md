---
description: Research the codebase, references, or anything else (GPT-5.4 optimized)
agent: NormalOpenAI
---

<task>
Research the user's topic thoroughly enough to produce a grounded research artifact with file:line evidence, clearly documented gaps, and actionable recommendations. Treat this as orchestration work: use sub-agents for discovery and deep reading, then use the main agent for planning, synthesis, and the final artifact. Do not jump to recommendations until the factual research is grounded.
</task>

<workflow_contract>
1. Follow all 7 steps in order.
2. Use `todowrite` starting in Step 2, keep exactly one item `in_progress`, and mark items complete immediately.
3. Prefer `tools_ask_agent` for exploration; the main agent should synthesize rather than manually brute-force the codebase.
4. Do not write the research document until coverage is sufficient or remaining gaps are explicitly documented.
5. If the request is too vague to research, ask a clarification question and stop there.
6. Do not stop on intermediate agent output; keep iterating until the research is ready for handoff.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Gather Context

1. Infer the concrete research objective from `<userMessage>`.
2. If the user named files, tickets, documents, or repositories, read each referenced file fully before doing anything else.
3. If there is no concrete research target after reading the user input, ask one clarification question and wait.
4. If the request is mostly clear but still has minor ambiguity, proceed with the best reasonable interpretation and state that assumption in your next user-facing update.
5. Call `tools_thoughts_list_documents`.
6. If Step 5 reveals relevant existing thoughts artifacts, note them for possible `tools_ask_agent` calls with `location=thoughts`. Do not read those documents directly in this step.
7. Call `tools_thoughts_list_references` and note which references may matter.

</step_1>

<step_2>

## Step 2: Decompose and Plan

1. Break the research target into sub-areas such as APIs, data models, configs, components, workflows, tests, and reference implementations.
2. Decide which sub-areas need:
   - `tools_ask_agent` with `agent_type=locator`
   - `tools_ask_agent` with `agent_type=analyzer`
   - `tools_ask_agent` with `location=references`
   - `tools_ask_agent` with `location=thoughts`
   - `tools_ask_agent` with `location=web`
3. Create a detailed `todowrite` plan that matches the actual research work.
4. Keep todos concrete and action-oriented.

**Good todo examples:**
- `Spawn locator for authentication-related files in src/`
- `Spawn analyzer for rate limiting flow across middleware and config`
- `Spawn reference analyzer on references/openai/codex for command prompt patterns`
- `Reflect on returned findings and decide whether another research pass is needed`
- `Write research artifact and sync it`

</step_2>

<step_3>

## Step 3: Launch Parallel Research

1. Launch independent `tools_ask_agent` calls in parallel in a single response block.
2. Start with a locator when you need to map unfamiliar terrain.
3. Launch 1-3 analyzers for distinct subsystems or layers.
4. Launch one analyzer per selected reference repository when external examples matter.
5. Use `location=thoughts` only if Step 1 found relevant existing documents.
6. Use `location=web` only when official docs or external behavior matters.
7. Ask every analyzer for file:line references.
8. The main agent should focus on orchestration and synthesis rather than doing all deep reading itself.

</step_3>

<step_4>

## Step 4: Reflect, Verify, and Resolve Gaps

1. Review all returned findings and decide what is now well-understood.
2. Identify what is still missing, uncertain, conflicting, or insufficiently grounded.
3. Do not fill gaps by speculation. Either investigate further or document the gap explicitly.
4. Use `tools_ask_reasoning_model` with `prompt_type="reasoning"` when:
   - agent findings conflict
   - a claim looks plausible but unverified
   - cross-cutting behavior spans multiple subsystems
   - two or more research passes still leave important gaps
   - file:line references contradict each other
5. When using `tools_ask_reasoning_model`, pass all relevant files/directories, not just the file currently in focus.
6. Give each file/directory a concise 1-2 sentence description of what it contains and why it matters.
7. Ask specific questions, request file:line support for code claims, and ask for alternatives or tradeoffs when the answer is not obvious.
8. After the reasoning-model response, cross-check important claims against tests, configs, or usage sites when possible.
9. Update `todowrite` immediately to reflect remaining research work or synthesis tasks.

</step_4>

<step_5>

## Step 5: Iterate Intentionally

1. Return to Step 2 if new sub-areas emerged, the research question changed, or your agent strategy needs to change.
2. Return to Step 3 if the plan is still sound and you just need targeted follow-up investigation.
3. Proceed to Step 6 only when coverage is sufficient and the remaining gaps are either resolved or explicitly documented.

</step_5>

<step_6>

## Step 6: Write the Research Document

The 2 targeted approaches and 2 comprehensive approaches should give the planning stage a real choice, not just fill a template slot.

1. Before writing, verify:
   - the todo list reflects the actual work
   - the key findings are grounded in file:line references
   - unresolved gaps are documented honestly
   - you have 2 targeted approaches and 2 comprehensive approaches with tradeoffs
2. Call `tools_thoughts_get_template` with `template=research`.
3. Write the document with `tools_thoughts_write_document` using:
   - `doc_type: "research"`
   - `filename: {readable_topic_name}.md` using only `A-Za-z0-9._-`
   - `content:` the completed research artifact following the template
4. The document must include:
   - the original request verbatim from `<userMessage>`
   - factual findings with file:line references
   - unresolved gaps or caveats
   - 2 targeted approaches with tradeoffs
   - 2 comprehensive approaches with tradeoffs
   - any useful iteration notes
5. After writing, execute `thoughts_sync` with `tools_cli_just_execute`. Do not use shell commands for sync.

</step_6>

<step_7>

## Step 7: Hand Off Clearly

1. Tell the user what you found, using the most important file:line references.
2. Call out unresolved gaps or risks briefly and explicitly.
3. Summarize the recommendations compactly.
4. Give the saved research path.
5. If they want to continue into planning, tell them to run `/create_plan_init {path/to/saved/research.md}`.
6. If they want more research instead, wait for direction.

</step_7>

</process>

<completion_gate>
You are done only when one of these is true:
1. You asked a necessary clarification question because the research target is still unclear.
2. You wrote and synced the research artifact, all relevant todos are complete, and the user received a grounded handoff with key findings, open gaps, recommendations, and the saved path.
</completion_gate>
