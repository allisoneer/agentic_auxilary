---
description: Post a structured design/scoping brief and questions to a Linear ticket, then stop
agent: Orchestrator
---

<task>
Read one Linear ticket and its full comment history, persist a ticket corpus artifact and a design/scoping brief artifact, perform bounded research only when needed, post a structured design/scoping brief + questions comment to Linear, and stop.
</task>

<workflow_contract>
1. Follow all 6 steps in order.
2. Use `todowrite` in Step 2 and keep exactly one todo `in_progress` at a time.
3. Treat `<userMessage>` as loose natural language with no first-pass flags or modifiers.
4. Require exactly one Linear ticket reference; ask and stop if it is missing or ambiguous.
5. Full ticket read means `linear_read_issue` plus repeated `linear_get_issue_comments` until `has_more=false`; stop fetching comments immediately after `has_more=false` and do not call again.
6. Before any synthesis or Linear posting, delegate full ticket intake, corpus export, and artifact-grounding handoff to one bounded Linear-capable child session. That delegated intake/export must:
   - filename: `linear-{lowercase-key}-ticket.md`
   - body includes `Ticket corpus schema: linear-ticket-corpus@v1` + snapshot timestamp + explicit snapshot disclaimer
   - include full ticket description plus full comment history with authorship and timestamps when available
   - write the artifact itself and run `thoughts_sync` immediately afterward
   - return enough grounded ticket identifiers plus the saved artifact path for the orchestrator to continue
7. This command is strictly Linear comment-only: post exactly one final Linear comment with `linear_add_comment`, and do not mutate Linear issue fields or perform any other external workflow mutations such as creating plans, implementing code, committing, pushing, or creating/updating PRs.
8. Duplicate runs may append fresh comments and create fresh design/scoping brief artifacts; the ticket corpus artifact should be refreshed by overwriting the stable `linear-{lowercase-key}-ticket.md` file.
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
   - ticket intake, corpus artifact persistence, and validation
   - design/scoping brief artifact creation and sync
   - bounded research if needed
   - final Linear comment posting
   - compact summary and stop
2. Keep the todos specific to the resolved ticket.

</step_2>

<step_3>

## Step 3: Read, Persist, and Validate the Full Ticket Corpus

1. Spawn one bounded Linear child session for ticket intake/export.
2. In that child session:
   - resolve the ticket reference
   - call `linear_read_issue` to read the issue details and description
   - call `linear_get_issue_comments` repeatedly with the same issue until `has_more=false`
   - stop immediately once `has_more=false`; do not issue another identical comments call
   - write the ticket corpus artifact itself via `thoughts_write_document(doc_type="artifact", filename=...)` using this shared contract:
     - Stable filename: `linear-{lowercase-key}-ticket.md`
     - Schema marker: `Ticket corpus schema: linear-ticket-corpus@v1`
     - Snapshot timestamp + disclaimer: point-in-time snapshot; may be stale
     - Include: ticket identifier, ticket title, ticket URL and current status if available, full description, full comments with authorship and timestamps when available, and a note that it is the authoritative ticket corpus for this run
   - run `thoughts_sync` immediately after writing
   - return the canonical ticket identifier, title, URL if available, current status if available, the saved artifact path, full description text, and confirmation that the full comment corpus was captured
3. Do not let the Linear child proceed into codebase research, planning, implementation, status changes, or comment posting.
4. If the ticket cannot be resolved responsibly, return the specific blocker to the user and stop.
5. Read the produced artifact back in the orchestrator session before proceeding.
6. Treat that read-back as a validation gate. Confirm the artifact contains:
   - the required schema marker
   - the ticket identifier and title
   - the ticket URL and current status if available
   - the snapshot timestamp and point-in-time disclaimer
   - the full description
   - the full comments/corpus with authorship and timestamps when available
7. Do not begin synthesis or Linear posting until this artifact exists and passes validation.

</step_3>

<step_4>

## Step 4: Create the Design/Scoping Brief Artifact and Exact Comment Body

1. Spawn a bounded `Normal` child session.
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

</step_4>

<step_5>

## Step 5: Post Exactly One Structured Linear Comment

1. Spawn a bounded Linear child session.
2. In that child session, post exactly one comment with `linear_add_comment` using the exact comment body prepared in Step 4.
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

</step_5>

<step_6>

## Step 6: Return a Compact Summary and Stop

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

</step_6>

</process>

<completion_gate>
You are done only when one of these is true:
1. You stopped immediately because no responsible Linear ticket reference could be determined from `<userMessage>`.
2. You stopped because the ticket reference was ambiguous and you asked one concise clarification question.
3. You completed Step 3 ticket intake/export validation, created and synced the design/scoping brief artifact, posted exactly one Linear comment, and returned a grounded summary with both artifact paths.
4. If the workflow stopped early due to a blocker, say exactly where it stopped and why.
</completion_gate>
