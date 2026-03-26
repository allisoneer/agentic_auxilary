---
description: Capture a structured OpenAI handoff artifact for later resumption
agent: NormalOpenAI
---

<task>
Create a self-contained handoff artifact for a future GPT-5.4 session. Keep workflow instructions and carried-forward context clearly separated inside the artifact so a fresh session can re-activate the correct command without losing progress state.
</task>

<workflow_contract>
1. Follow all 6 steps in order.
2. Prefer explicit state over vague narrative.
3. Distinguish confirmed facts, settled decisions, and tentative/open items.
4. If you can identify the current workflow command, record it explicitly and re-read that command file before writing the artifact.
5. The artifact must be sufficient for a fresh session that knows only the repo, system prompts, and this artifact.
6. Validation claims must be labeled `confirmed`, `inferred`, or `unconfirmed`.
7. If workflow prompt files or `AGENTS.md` changed in this session, record whether a post-write comparison pass already happened or is still required.
8. If uncertainty remains, label it explicitly instead of smoothing it over.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Identify Scope and Workflow

1. Infer the current task and handoff scope from the active session plus `<userMessage>`.
2. Treat additional user text as handoff focus, resume emphasis, or overrides.
3. Determine whether the current work is primarily associated with a known workflow command such as:
   - `.opencode/command/implement_plan_openai.md`
   - `.opencode/command/research_openai.md`
   - `.opencode/command/create_plan_init_openai.md`
   - `.opencode/command/create_plan_final_openai.md`
   - `.opencode/command/review_pr_comments_openai.md`
4. If a known workflow is identified, record:
   - `source_workflow_command` = slash-command form such as `/implement_plan_openai`
   - `source_workflow_file` = repo path such as `.opencode/command/implement_plan_openai.md`
   - `resume_mode` = one of `continue_source_workflow`, `continue_generic_work`, or `ask_user_first`
5. If the workflow is unclear, fall back to a generic handoff and say so explicitly in the artifact.

</step_1>

<step_2>

## Step 2: Refresh Only the Critical Context

1. Re-read the source workflow command file if Step 1 identified one.
2. Re-read the minimum anchor documents needed to make the artifact accurate.
3. For plan-driven work, make sure you know:
   - requirements document path
   - implementation plan path
   - current phase or next unfinished item
   - completed phases or milestones
   - verification status
   - meaningful divergences from the original plan
4. For research or planning work, make sure you know the latest research, open questions, and recommended next command.
5. If the current session edited `.opencode/command/*_openai.md` files or `AGENTS.md`, re-read those changed files and note whether they already received a post-write comparison or external review.
6. Do not do a broad new investigation here. Reload only the files and documents needed for an accurate handoff.

</step_2>

<step_3>

## Step 3: Extract the State That Must Survive

1. Capture the goal and success condition.
2. Capture the current state of the work.
3. Capture completed work.
4. Capture files changed, explicitly marking workflow prompt files and `AGENTS.md` when they are part of the change set.
5. Capture files researched or anchor documents.
6. Capture settled decisions and why they were made.
7. Capture open questions, risks, and tentative areas.
8. Capture remaining work in priority order.
9. Capture validation status and unfinished verification, labeling each item as `confirmed`, `inferred`, or `unconfirmed`.
10. Capture failed paths or things already tried.
11. Capture the exact first actions the next session should take, including any required post-write comparison pass for workflow prompt files or `AGENTS.md`.

</step_3>

<step_4>

## Step 4: Write a Structured Handoff Artifact

1. Call `tools_thoughts_list_documents` and look for an existing matching handoff artifact for the same task.
2. If an existing handoff artifact is clearly the same task, update it.
3. If the source workflow is implement_plan-like, prefer a filename that starts with `plan_{basename}_` so existing implement_plan artifact discovery can still find it.
4. Otherwise create `handoff_{readable_task_name}.md`.
5. Draft the content so it already includes an `## Exact Resume Prompt` section, even if you must temporarily use a placeholder for the final saved path.
6. Write the artifact using `tools_thoughts_write_document` with `doc_type="artifact"`.
7. If the write result gives you a more precise saved path than the draft content used, immediately update the same artifact so the stored resume prompt contains the exact path.
8. The artifact must use these exact headings:

```md
# OpenAI Work Handoff
## Resume Protocol
### Source Workflow
### Resume Mode
### Recommended Resume Command
### Recommended Agent
### Anchor Documents
### First Actions for Next Session
### Do Not Redo
## Goal and Success Condition
## Current State
## Completed Work
## Files Changed
## Files Researched / Wider Context
## Settled Decisions
## Open Questions / Risks
## Remaining Work (Priority Order)
## Validation Status
## Failed Paths / Things Already Tried
## Exact Resume Prompt
```

9. Under `## Resume Protocol`, include:
   - inside `### Source Workflow`, write labeled fields for both `source_workflow_command` and `source_workflow_file`
   - `recommended_resume_command`
   - `recommended_agent`
   - `resume_mode`
10. Under `### Anchor Documents`, list only the documents the next session must read first.
11. Under `## Files Researched / Wider Context`, list broader context files that informed the work but are not mandatory first reads.
12. Under `## Validation Status`, make the `confirmed` versus `inferred` versus `unconfirmed` distinction explicit for every verification claim.
13. Under `## Exact Resume Prompt`, write a copy-paste prompt that tells a fresh session to use `/resume_work_openai {artifact_path}`.

</step_4>

<step_5>

## Step 5: Sync and Verify the Artifact

1. Execute `thoughts_sync` using `tools_cli_just_execute`.
2. Before finishing, verify that the artifact is self-contained:
    - a fresh session can understand the goal
    - a fresh session can tell what is already done
    - a fresh session can tell what to do next
    - workflow instructions and carried-forward context are clearly separated
3. If the work is implement_plan-like, verify that the artifact explicitly names the source plan files, current phase or next unfinished item, and outstanding verification.
4. If workflow prompt files or `AGENTS.md` changed, verify the artifact says whether post-write comparison already happened or is still required next session.
5. If anything is uncertain, label it inside the artifact instead of pretending it is settled.

</step_5>

<step_6>

## Step 6: Hand Off to the User

1. Give the saved artifact path.
2. Summarize the goal, current state, and immediate next action.
3. Recommend resuming with `/resume_work_openai {artifact_path}`.
4. If the source workflow is known, say which workflow the next session should re-activate.

</step_6>

</process>

<completion_gate>
You are done only when the handoff artifact is written, synced, verified as self-contained, any workflow prompt-file or `AGENTS.md` comparison status is recorded, and the user has the saved path plus the recommended resume command.
</completion_gate>
