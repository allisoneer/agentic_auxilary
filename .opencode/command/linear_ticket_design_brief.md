---
description: Post a structured design/scoping brief and questions to a Linear ticket, then stop
agent: OrchestratorOpenAI
---

<task>
Read one Linear ticket and its full comment history, persist a ticket corpus artifact and a design/scoping brief artifact, perform bounded research only when needed, post a structured design/scoping brief + questions comment to Linear, and stop.
</task>

<workflow_contract>
1. Follow all 7 steps in order.
2. Use `todowrite` in Step 2 and keep exactly one todo `in_progress` at a time.
3. Treat `<userMessage>` as loose natural language with no first-pass flags or modifiers.
4. Require exactly one Linear ticket reference; ask and stop if it is missing or ambiguous.
5. Full ticket read means `linear_read_issue` plus repeated `linear_get_issue_comments` until `has_more=false`; stop fetching comments immediately after `has_more=false` and do not call again.
6. Persist the authoritative ticket corpus artifact before posting to Linear, and run `thoughts_sync` after each artifact write.
7. This command is strictly comment-only: post exactly one final Linear comment with `linear_add_comment`, and do not mutate issue fields, create plans, implement code, commit, push, or create/update PRs.
8. Duplicate runs may append fresh comments and create fresh artifacts; do not attempt comment updates or deduplication.
9. Bounded research is allowed only when needed to avoid guessing.
10. Hard stop immediately after posting the Linear comment and returning the compact summary.
</workflow_contract>

<userMessage>
$ARGUMENTS
</userMessage>

<process>

<step_1>

## Step 1: Resolve Exactly One Ticket Reference

1. Parse `<userMessage>` as loose natural language.
2. Accept any of these as sufficient ticket references:
   - a Linear issue key such as `ENG-123`
   - a Linear issue URL
   - another identifiable Linear issue reference that a bounded Linear child session can resolve responsibly
3. If no Linear ticket reference is present, ask the user for one and stop.
4. If multiple plausible tickets are referenced and one is not clearly primary, ask one concise clarification question and stop.

</step_1>

<step_2>

## Step 2: Build the Orchestrator Todo List

1. Create concrete todos for:
   - ticket intake
   - ticket corpus artifact persistence and sync
   - design/scoping brief artifact creation and sync
   - bounded research if needed
   - final Linear comment posting
   - compact summary and stop
2. Keep the todos specific to the resolved ticket.

</step_2>

<step_3>

## Step 3: Read the Full Ticket and Comment History

1. Spawn a bounded Linear child session.
2. In that child session:
   - resolve the ticket reference
   - call `linear_read_issue` to read the issue details and description
   - call `linear_get_issue_comments` repeatedly with the same issue until `has_more=false`
   - stop immediately once `has_more=false`; do not issue another identical comments call
   - return the canonical ticket identifier, title, URL if available, current status if available, full description text, and full comment corpus with authorship and timestamps when available
3. Do not let the Linear child proceed into codebase research, planning, implementation, status changes, or comment posting.
4. If the ticket cannot be resolved responsibly, return the specific blocker to the user and stop.

</step_3>

<step_4>

## Step 4: Persist the Authoritative Ticket Corpus Artifact

1. Spawn a bounded `NormalOpenAI` child session whose only job is to write the ticket corpus artifact under thoughts and sync it.
2. Provide that child the full ticket output from Step 3 and require an artifact that includes:
   - ticket identifier
   - ticket title
   - ticket URL and current status if available
   - full description
   - full comments with authorship and timestamps when available
   - a note that it is the authoritative ticket corpus for this run
3. Require the child to run `thoughts_sync` with `tools_cli_just_execute` after writing.
4. Read the produced artifact back in the orchestrator session before proceeding.
5. Do not begin synthesis or Linear posting until this artifact exists.

</step_4>

<step_5>

## Step 5: Create the Design/Scoping Brief Artifact and Exact Comment Body

1. Spawn a bounded `NormalOpenAI` child session.
2. Give it the ticket corpus artifact path plus the original ticket intake.
3. Its only responsibilities are:
   - synthesize the current understanding grounded in the ticket corpus
   - explain why the ticket is underspecified
   - identify possible design directions with tradeoffs
   - identify key decision-level questions
   - suggest a scope boundary and follow-up work
   - recommend the next state
   - draft the exact final Linear comment body verbatim
   - write the design/scoping brief artifact under thoughts and sync it
4. The design/scoping brief artifact must include:
   - Current understanding
   - Why the ticket is underspecified
   - Possible design directions with tradeoffs
   - Key decision-level questions
   - Suggested scope boundary / follow-ups
   - Recommended next state
   - Exact Linear comment body (verbatim)
   - Artifact links/paths
5. Allow bounded additional research only when needed to avoid guessing. Any such research must stay limited to identifying likely affected code areas, constraints, risks, or unresolved questions needed for the brief.
6. Require the child to run `thoughts_sync` with `tools_cli_just_execute` after writing.
7. Read the resulting brief artifact in the orchestrator session and extract the exact final Linear comment body from it.
8. Do not create or invoke planning, implementation, commit, or PR workflows.

</step_5>

<step_6>

## Step 6: Post Exactly One Structured Linear Comment

1. Spawn a bounded Linear child session.
2. In that child session, post exactly one comment with `linear_add_comment` using the exact comment body prepared in Step 5.
3. The comment should be structured for the ticket author and include, in concise form:
   - Current understanding
   - Why the ticket is underspecified
   - Possible design directions with tradeoffs
   - Key decision-level questions
   - Suggested scope boundary / follow-ups
   - Recommended next state
   - References to the saved artifact paths when appropriate
4. Do not call `linear_update_issue`, do not edit prior comments, and do not perform any other Linear mutation.
5. If posting fails, return the exact blocker and stop without attempting alternate mutations.

</step_6>

<step_7>

## Step 7: Return a Compact Summary and Stop

1. Return a compact summary that includes:
   - resolved ticket identifier
   - ticket URL if available
   - ticket corpus artifact path
   - design/scoping brief artifact path
   - whether bounded research was needed
   - Linear comment posting status
   - any blockers or caveats
2. State clearly that the workflow stopped after posting the comment.
3. Do not continue into planning, implementation, verification, commit, push, PR creation, or any other downstream workflow.

</step_7>

</process>

<completion_gate>
You are done only when one of these is true:
1. You stopped immediately because no responsible Linear ticket reference could be determined from `<userMessage>`.
2. You stopped because the ticket reference was ambiguous and you asked one concise clarification question.
3. You read the ticket, persisted the authoritative ticket corpus artifact, created and synced the design/scoping brief artifact, posted exactly one Linear comment, and returned a grounded summary with both artifact paths.
4. If the workflow stopped early due to a blocker, say exactly where it stopped and why.
</completion_gate>
