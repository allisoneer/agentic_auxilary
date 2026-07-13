---
description: Interactive conventional-commit Bash workflow that reviews diff/status, proposes atomic commits, and asks before committing.
agent: Bash
---
# Commit Changes

You are tasked with creating git commits for the changes made during this session.

**User message (if any):** $ARGUMENTS

## Process:

1. **Think about what changed:**
   - Review the conversation history and understand what was accomplished
   - Run `git status` to see current changes
   - Run `git diff` to understand the modifications
   - Consider whether changes should be one commit or multiple logical commits

2. **Determine commit type:**
   Analyze the changes and categorize them:
   - `feat`: New feature or functionality added
   - `fix`: Bug fix or error correction
   - `refactor`: Code restructuring without changing functionality
   - `docs`: Documentation only changes
   - `test`: Adding or updating tests
   - `chore`: Maintenance tasks (deps, configs, etc.)
   - `perf`: Performance improvements
   - `style`: Formatting, missing semicolons, etc.
   - `ci`: CI/CD configuration changes

3. **Plan your commit(s) with conventional format:**
   ```
   <type>(<scope>): <description>
   
   [optional body]
   
   [optional footer(s)]
   ```
    - Scope is optional—use a short component, package, or directory name when it adds clarity
    - For cross-cutting changes, use generic scopes like `build`, `deps`, `ci`, or omit the scope entirely
   - Description should be imperative mood, lowercase, no period
   - Body explains WHY, not what (the diff shows what)
   - Footer can include `BREAKING CHANGE:` for major version bumps

4. **Present your plan to the user:**
   - List the files you plan to add for each commit
   - Show the conventional commit message(s) you'll use
   - Ask: "I plan to create [N] commit(s) with these changes. Shall I proceed?"

5. **Execute upon confirmation:**
   - Use `git add` with specific files (never use `-A` or `.`)
   - Create commits with your planned messages
   - Show the result with `git log --oneline -n [number]`

## Examples:

- `feat(auth): add oauth callback handler`
- `fix(cli): handle missing config file gracefully`
- `refactor(api): extract common request parsing`
- `docs: document local dev setup`
- `chore(deps): bump tokio to 1.40`

## Important:
- **NEVER add co-author information or Claude attribution**
- Commits should be authored solely by the user
- Do not include any "Generated with Claude" messages
- Do not add "Co-Authored-By" lines
- Write commit messages as if the user wrote them
- **ALWAYS use conventional commit format for automated versioning**

## Breaking Changes:
If a change breaks backward compatibility, add `BREAKING CHANGE:` in the footer:
```
feat(api): change response format to JSON

BREAKING CHANGE: API responses now return JSON instead of plain text.
Consumers must update their parsing logic.
```

## Remember:
- You have the full context of what was done in this session
- Group related changes together
- Keep commits focused and atomic when possible
- Use conventional commits to enable automated versioning
