---
description: Generate a PR Description
---

# Generate PR Description

You are tasked with generating a comprehensive pull request description following the repository's standard template.

**User message (if any):** $ARGUMENTS

## Steps to follow:

1. **Get the PR description template:**
   - Call `thoughts_get_template` with `template=pr_description`
   - Read the template carefully to understand all sections and requirements

2. **Identify the PR to describe:**
   - Check if the current branch has an associated PR: `gh pr view --json url,number,title,state 2>/dev/null`
   - If no PR exists for the current branch, or if on main/master, list open PRs: `gh pr list --limit 10 --json number,title,headRefName,author`
   - Ask the user which PR they want to describe

3. **Check for existing description (MCP):**
   - Call `thoughts_list_active_documents` and filter `doc_type = "artifact"` with filename `pr_{number}_description.md`
   - If found, read it and inform user you'll be updating it
   - Consider what has changed since last description

4. **Gather comprehensive PR information:**
   - Get the full PR diff: `gh pr diff {number}`
   - If you get an error about no default remote repository, instruct the user to run `gh repo set-default` and select the appropriate repository
   - Get commit history: `gh pr view {number} --json commits`
   - Review the base branch: `gh pr view {number} --json baseRefName`
   - Get PR metadata: `gh pr view {number} --json url,title,number,state`

5. **Analyze the changes thoroughly:** (think deeply about the code changes, their architectural implications, and potential impacts)
   - Read through the entire diff carefully
   - For context, read any files that are referenced but not shown in the diff
   - Understand the purpose and impact of each change
   - Identify user-facing changes vs internal implementation details
   - Look for breaking changes or migration requirements

6. **Handle verification requirements:**
   - Look for any checklist items in the "How to verify it" section of the template
   - For each verification step:
     - If it's a command you can run (like `just check test`, `npm test`, etc.), run it
     - If it passes, mark the checkbox as checked: `- [x]`
     - If it fails, keep it unchecked and note what failed: `- [ ]` with explanation
     - If it requires manual testing (UI interactions, external services), leave unchecked and note for user
   - Document any verification steps you couldn't complete

7. **Generate the description:**
   - Fill out each section from the template thoroughly:
     - Answer each question/section based on your analysis
     - Be specific about problems solved and changes made
     - Focus on user impact where relevant
     - Include technical details in appropriate sections
     - Write a concise changelog entry
   - Ensure all checklist items are addressed (checked or explained)

8. **Save and sync the description (MCP):**
   - Call `thoughts_write_document`:
     - `doc_type`: "artifact"
     - `filename`: `pr_{number}_description.md`
     - `content`: completed description from template
   - Run `thoughts sync` with bash in current directory

9. **Update the PR:**
   - After thoughts sync, the file will be at a path in thoughts/active/{branch}/artifacts/
   - Update PR description: `gh pr edit {number} --body-file {path_returned_from_sync}`
   - Confirm update successful
   - If any verification steps remain unchecked, remind user to complete them before merging

## Important notes:
- This command works across different repositories - always read the local template
- Be thorough but concise - descriptions should be scannable
- Focus on the "why" as much as the "what"
- Include any breaking changes or migration notes prominently
- If the PR touches multiple components, organize the description accordingly
- Always attempt to run verification commands when possible
- Clearly communicate which verification steps need manual testing
