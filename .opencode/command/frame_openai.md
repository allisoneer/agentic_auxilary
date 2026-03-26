---
description: Turn a short or under-specified prompt into a stronger GPT-5.4 working frame
model: opencode/gpt-5.4
---

<task>
Receive a short, vague, or under-specified user prompt and turn it into a stronger working frame before acting. Your job is to infer the real task shape, surface hidden constraints and likely success criteria, decide whether targeted tool use is necessary, and then either proceed directly or recommend the best downstream workflow command.

For this command, the requested work is the framing itself. Producing a grounded frame and either routing cleanly or proceeding with bounded work counts as completion.
</task>

<workflow_contract>
1. Follow all 6 steps in order.
2. Treat the user prompt as meaningful even if it is short.
3. Distinguish clearly between confirmed facts, inferred framing, and open questions.
4. Continue tool use until the frame is grounded enough to route or act responsibly; for this command, that grounded frame is the completion target unless direct execution mode is chosen.
5. Prefer `tools_ask_agent` for grounded discovery when codebase facts matter.
6. If a specialized workflow command is clearly the better fit, recommend it explicitly instead of forcing direct execution.
7. If the request is bounded enough to answer or execute directly after framing, proceed directly.
8. Ask a clarification question only when the prompt lacks enough subject matter to frame responsibly.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Parse the Prompt and Classify It

1. Read `<userMessage>` carefully and infer the most likely explicit request.
2. Infer the likely deeper objective behind the wording when one is apparent.
3. Classify the prompt across these axes:
   - task type: question | research | planning | implementation | review | resume | handoff | mixed
   - scope: single file | subsystem | cross-cutting | repo-wide | unknown
   - ambiguity: fully unclear | partially specified | mostly clear
4. If the prompt names files, documents, commands, PRs, or plans, record them as anchor points.
5. If the prompt has too little subject matter to frame responsibly, ask one clarification question and stop.

</step_1>

<step_2>

## Step 2: Decide Whether Grounding Tools Are Needed

1. If the prompt already contains enough context to frame the work responsibly, skip to Step 4.
2. If the frame depends on codebase facts, file locations, workflow artifacts, or repository conventions, decide which minimal tools would reduce guessing.
3. Use `todowrite` if the framing pass is complex or non-trivial, requires 3 or more concrete actions, multiple tool calls, or a direct continuation into real work.
4. If the prompt plausibly touches ongoing branch work, prior research, saved plans, or handoff state, call `tools_thoughts_list_documents` rather than guessing whether relevant artifacts exist.
5. If the prompt may depend on reference examples already available in the repo, call `tools_thoughts_list_references` rather than guessing.
6. If the framing depends on codebase discovery, prefer `tools_ask_agent` over manual grep/glob exploration.

</step_2>

<step_3>

## Step 3: Ground the Frame with Minimal Necessary Investigation

1. If file or symbol discovery is needed, use `tools_ask_agent` with `agent_type=locator`.
2. If subsystem behavior or data flow understanding is needed, use `tools_ask_agent` with `agent_type=analyzer`.
3. If the prompt implies an ongoing workstream, use `tools_thoughts_list_documents` and read only the anchor artifacts that materially affect the frame.
4. If independent discovery tasks exist, launch them in parallel in a single response block.
5. Use `tools_ask_reasoning_model` with `prompt_type="reasoning"` when:
   - findings conflict
   - a claim looks plausible but unverified
   - cross-cutting tradeoffs remain unresolved
   - two or more grounding passes still leave important gaps
   - the best route is still unclear after targeted discovery
6. Keep this investigation intentionally narrow. Gather just enough evidence to stop guessing.

</step_3>

<step_4>

## Step 4: Build the Working Frame

1. Construct the frame explicitly using these buckets:
   - confirmed request
   - likely deeper objective
   - inferred scope
   - confirmed constraints
   - inferred constraints or assumptions
   - success condition
   - biggest risks or unknowns
2. If some parts are inferred rather than confirmed, label them as inferred.
3. If important uncertainty remains but the task is still actionable, preserve it as an explicit caveat rather than stopping.
4. If ambiguity is meaningful, compare 2 plausible frames briefly before choosing one.
5. Decide the best execution mode:
   - question → answer directly or recommend `/research_openai`
   - research → recommend `/research_openai`
   - planning → recommend `/create_plan_init_openai`
   - implementation with an approved plan → recommend `/implement_plan_openai`
   - review of PR comments → recommend `/review_pr_comments_openai`
   - resume of interrupted work → recommend `/resume_work_openai`
   - handoff or context extraction → recommend `/unwind_openai`
   - bounded ad hoc work that does not clearly fit a specialized workflow → continue directly
   - mixed → choose the dominant unresolved need and say why
6. A strong frame should usually look like:

```md
What I think you mean:
- ...

What seems implied:
- ...

What I need to ground:
- ...

Best next move:
- ...
```

</step_4>

<step_5>

## Step 5: Either Proceed or Route Cleanly

### If direct execution is appropriate

1. Tell the user briefly how you are interpreting the task.
2. State any important assumptions.
3. Then proceed directly with the work.

### If a specialized workflow command is the better fit

1. Tell the user how you are interpreting the task.
2. State the best next command and why it fits better than ad hoc execution.
3. If you created or identified an artifact path that should be passed forward, include the exact command invocation.
4. Do not continue into broad direct work after recommending a better workflow unless the user explicitly wants that.

### If the prompt is primarily a framing or interpretation request

1. Return the strongest framed interpretation you can support.
2. Include confirmed facts, inferred framing, and open questions separately.
3. End with the best next move.

</step_5>

<step_6>

## Step 6: Final Response Shape

1. Before finalizing, verify:
   - confirmed facts came from the user message or tool outputs
   - inferred framing is labeled as inferred
   - important unknowns are explicit rather than hidden
   - the chosen execution mode matches the frame
2. Keep the response concise, but include enough structure to show the result of the framing work.
3. For simple prompts, compress the frame into a few high-signal bullets. For more ambiguous prompts, use short labeled sections if that improves clarity.
4. At minimum, include:
   - what you think the user means
   - what matters most
   - what you are doing next or what command they should run next
5. If you used tools, make sure the final framing is grounded in what those tools returned.
6. If you are routing to another command, provide the exact command invocation in backticks.

</step_6>

</process>

<completion_gate>
You are done only when one of these is true:
1. You asked one necessary clarification question because the prompt lacked enough subject matter to frame responsibly.
2. You provided a grounded working frame, verified that it was grounded and clearly labeled, and then proceeded directly with the bounded work.
3. You provided a grounded working frame and routed the user to the best next workflow command with an exact invocation.
4. You provided a grounded framed interpretation because the request itself was primarily about interpretation rather than execution.
</completion_gate>
