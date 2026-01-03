---
description: Research the codebase, references, or anything else
---

<task>
You are conducting comprehensive codebase research to gather factual information about a topic or question. Your goal is to explore the codebase, discover relevant code and patterns, synthesize what you find, and document it thoroughly for future use.

**Important**: Focus on verifiable facts with file:line references. Document what exists, where it exists, and how it works. Recommendations come after thorough factual research. Use todowrite to track your progress through the research process.
</task>

**MAKE SURE** you follow ALL 7 steps in the process, EXACTLY as you are instructed to, AND track each of the steps with your todo list to make sure you stay on track.

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step name="context_gathering" id="1">

## Context Gathering

**User Input:**
- If the user provided parameters (files, tickets, research question) in `<userMessage>`, read all referenced files FULLY (no limit/offset).
- Gain an understanding of the desired topic to research.
- If no clear research question was provided, ask for clarification and wait.

**Thoughts and References:**
1. Call `thoughts_list_active_documents` to see what exists. If relevant artifacts are found, keep them in mind for potential investigation via `tools_spawn_agent` with `location=thoughts` during research—do not read them directly here.
2. Call `thoughts_list_references` to identify which external references may be useful for this research topic.

</step>

<step name="decompose_and_plan" id="2">

## Decompose and Plan

Break the research question into sub-areas (APIs, data models, configuration, components, patterns). Decide on your `tools_spawn_agent` strategy:
- Locators (`agent_type=locator`): Find where things live
- Analyzers (`agent_type=analyzer`): 1–3 for distinct subsystems or components
- Reference analysis (`agent_type=analyzer`, `location=references`): Per selected reference
- Web knowledge (`location=web`): When external docs are needed

**Create Your Todo List:**

Now use `todowrite` to create a detailed research plan. This is your active scratch pad—not generic step tracking. Be specific about what you will actually do:

- **Good example todos:**
  - "Spawn locator to find all authentication-related files in src/"
  - "Spawn analyzer for the database layer focusing on connection pooling"
  - "Spawn analyzer on references/org/repo for their rate limiting patterns"
  - "Spawn web analyzer to find official docs on X library's retry behavior"
  - "Reflect on agent results, identify gaps, and decide if another iteration of research is necessary"
  - "Write research document"

- **Bad example todos:**
  - "Do step 3"
  - "Research the codebase"
  - "Spawn some agents"

Each todo should be actionable and specific enough that you know exactly what to do when you reach it.

</step>

<step name="parallel_research" id="3">

## Spawn Parallel Agents

Launch agents concurrently (in parallel) using `tools_spawn_agent`:

- Start with a locator to map the landscape.
- Spawn 1-3 analyzers for distinct subsystems or layers.
- Spawn 1 analyzer per selected reference.
- Use `location=thoughts` only if Step 1 found relevant existing documents.

**Guidance**:
- Be specific about directories and what to extract. Request file:line references.
- Main agent focuses on orchestration and synthesis; sub-agents do deep, parallel reading.
- Prefer multiple concurrent agents to maximize coverage efficiently.

</step>

<step name="reflection" id="4">

## Reflection

Review everything returned from your agents. Think critically about what you've learned and what's still missing.

**Assess Coverage:**
- What areas of the research question are now well-understood?
- What areas still have gaps or insufficient detail?
- Are there new sub-areas that emerged from the research that weren't in your original plan?

**Evaluate Quality:**
- Do any findings seem "sketchy"—they look right but feel uncertain or unverified?
- Are there conflicting answers from different agents or sources?
- Do any file:line references contradict each other?

**Consider Using `reasoning_model_request`:**
When you encounter uncertainty that agents can't resolve, use the reasoning model for a second opinion:
- Conflicting information from multiple agents
- Findings that look correct but feel suspicious
- Complex cross-cutting concerns spanning multiple subsystems
- You've done 2+ research iterations and gaps persist
- Contradictory file:line references that need disambiguation

When calling, provide all relevant files with concise descriptions of what each contains and why it matters.

**Update Your Todos:**
Based on your reflection, update your todo list with any remaining research work:
- Additional agent calls needed (and to which locations)
- Specific questions for the reasoning model
- Areas ready for synthesis

</step>

<guidance name="using_reasoning_model">

## Using the Reasoning Model Effectively

When you decide to use `reasoning_model_request`, here's how to get the most value:

**Provide Context Well:**
- Include ALL relevant files, not just the one you're focused on
- For each file/directory, write 1-2 sentences explaining what it contains and why it matters for your question
- Use directories when many related files exist; set `extensions` and `recursive` appropriately

**Frame Your Request:**
- Be specific about what's uncertain and why
- Request file:line references for any code-related claims
- Ask it to enumerate possibilities when multiple explanations exist, with evidence for each

**Good Prompts:**
- "These two files seem to contradict each other on X. Can you clarify which is authoritative and why?"
- "I found this pattern in 3 places but I'm not confident I understand the invariant. What constraints does this impose?"
- "Given these interfaces, what are the actual data flow paths and where might Y occur?"
- "Based on these findings, what are some targeted vs comprehensive approaches to address this? Include tradeoffs for each."

**After Getting a Response:**
- Cross-validate claims against tests, configs, or usage sites when possible
- If still uncertain, that's useful information—note it as an open gap

</guidance>

<step name="iteration" id="5">

## Iteration

Check your todo list. If there's remaining research work, continue iterating.

**If todos have more research to do:**

Return to the appropriate earlier step based on what's needed:

- **Go to Step 2 (Decompose and Plan)** when:
  - New sub-areas emerged that weren't in your original plan
  - The research question has evolved based on what you learned
  - You need to fundamentally rethink your agent strategy

- **Go to Step 3 (Parallel Research)** when:
  - You just need more agent calls on already-identified areas
  - You're filling in specific gaps with targeted queries
  - You're calling the reasoning model for clarification

**If research is complete:**

When your reflection shows sufficient coverage and your todos have no remaining research items, proceed to Step 6 (Write Document).

</step>

<step name="write_document" id="6">

## Write Research Document

**Before Writing — Consider Recommendations:**

Think about what recommendations you can offer. The research document should include a Recommendations section with:
- **2 Targeted Approaches**: Focused fixes that address the immediate problem with minimal scope. Include tradeoffs (what you gain vs what you defer).
- **2 Comprehensive Approaches**: Broader solutions that address root causes or adjacent concerns. Include tradeoffs (what you gain vs the cost in effort/complexity).

Neither category is inherently better—the right choice depends on context. Present both honestly so the planning stage can make an informed decision.

**Write the Document:**

1. Call `thoughts_get_template` with `template=research` to get the exact structure and tone.

2. Write the document using `thoughts_write_document`:

**Parameters:**
- **doc_type**: "research"
- **filename**: `{readable_topic_name}.md` (A–Z, a–z, 0–9, dot, underscore, hyphen only; no slashes)
- **content**: The completed research per the template, including:
  - Original Request (verbatim from `<userMessage>`)
  - Factual findings with file:line references
  - Recommendations (2 targeted + 2 comprehensive approaches, each with tradeoffs)
  - Any iteration comments

3. After writing, sync via the Just MCP tools: execute the "thoughts_sync" recipe with `tools_just_execute`. Do not run shell commands directly.

</step>

<step name="handoff" id="7">

## Handoff

Provide a summary of your research findings to the user:
- Key discoveries and their locations (file:line references for the most important items)
- Any unresolved gaps or areas that need further investigation
- Your recommendations (brief summary of the 2+2 approaches)

Then let the user know their options:
- If they want to proceed to planning: "Run `/create_plan_init {path/to/saved/research.md}` to start creating an implementation plan."
- If they have follow-up questions or want more research, wait for their direction.

</step>

</process>
