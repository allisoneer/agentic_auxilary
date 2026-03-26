# Visual Guide to Allison's Agent Workflows

**Allison** (@allisonology) · Mar 10 · 9 tweets

> All MIT licensed. Helps you actually get a picture of what I'm building towards, and how nice it already is.

---

## Level 0: ORCHESTRATOR (Parent Agent)

First is orchestration level, allowing full management of opencode sessions.

```
LEVEL 0: ORCHESTRATOR (Parent Agent)
├── orchestrator_run              - Spawn/resume sessions
├── orchestrator_list_sessions    - List active sessions
├── orchestrator_list_commands    - List available commands
├── orchestrator_respond_permission - Handle permission requests
├── read                          - Inspect files for coordination
└── todowrite                     - Task tracking
                        │
                        │  spawns via command
                        ▼
```

---

## Level 1: SESSION AGENTS (Command-Based)

Then is what I used to refer to as "primary agent" level. Not so primary anymore, but still this is a level that can always be resumed/forked by a human, even if spawned/managed via orchestrator initially. "Normal" is what I use the majority of the time.

Inside of normal we have a handful of tools, most are custom, and pluggable into any MCP client via a single tool exposing all of them. `cargo install agentic-mcp`. I don't use bash by default, and I find I rarely need it, most of the agent execution is handled via justfile.

### Agent Variants

| NORMAL (19)   | BASH (20)     | LINEAR (25)   | PLAYWRIGHT (37)       | REVIEW (9)            |
|---------------|---------------|---------------|-----------------------|-----------------------|
| File ops      | File ops      | File ops      | File ops              | Read only             |
| Search        | Search        | Search        | Search                | Search (cli_*)        |
| Tasks         | Tasks         | Tasks         | Tasks                 |                       |
| Just runner   | Just runner   | Just runner   | Just runner           | Just runner (limited) |
| Thoughts      | Thoughts      | Thoughts      | Thoughts              | Thoughts (write only) |
| GitHub PRs    | GitHub PRs    | GitHub PRs    | GitHub PRs            |                       |
| Sub-agents    | Sub-agents    | Sub-agents    | Sub-agents            | Reasoning model       |
|               | + mcp_bash (shell) | + 6 Linear tools | + 18 Browser automation | + review_* tools |

### Commands Using Each

```
NORMAL:     research, create_plan_init, create_plan_final, implement_plan,
            review_pr_comments, init
BASH:       bash, commit, describe_pr
LINEAR:     linear
PLAYWRIGHT: playwright
REVIEW:     review
```

```
                        │
                        │  can spawn via ask_agent / ask_reasoning_model
                        ▼
```

---

## Level 1: Session Agent Tools

### Base Toolset (19 tools - all agents have these)

| Category    | Tool                     | Key Parameters                                        |
|-------------|--------------------------|-------------------------------------------------------|
| **File Ops**| `read`                   | filePath, offset, limit                               |
|             | `edit`                   | filePath, edits[] (exact replacements)                |
|             | `write`                  | filePath, content                                     |
| **Search**  | `grep`                   | regex, path, mode (files/content/count)               |
|             | `glob`                   | pattern, sort                                         |
|             | `ls`                     | path, depth, filter                                   |
| **Tasks**   | `todowrite`              | todos[] with status/priority                          |
| **Just**    | `just_execute`           | recipe, args[]                                        |
|             | `just_search`            | query                                                 |
| **Thoughts**| `thoughts_write_document`| path, content                                         |
|             | `thoughts_list_documents`| -                                                     |
|             | `thoughts_get_template`  | template_type (research/plan/requirements/pr_description) |
|             | `thoughts_add_reference` | github_url                                            |
|             | `thoughts_list_references`| -                                                    |
| **GitHub**  | `gh_get_prs`             | state, limit                                          |
|             | `gh_get_comments`        | pr_number                                             |
|             | `gh_add_comment_reply`   | comment_id, body                                      |
| **Sub-agents**| `ask_agent`            | agent_type, location, query                           |
|             | `ask_reasoning_model`    | prompt, files[], prompt_type                          |

If I ever need additional tools that aren't in the list of ones above, I'll tab over to a different agent that has access to them. Also, sometimes commands will automatically bring those tools into context just for the duration of the command being ran, e.g. `/commit`

---

## Agent-Specific Additional Tools

### Bash Agent (+1 tool = 20 total)

| Tool       | Parameters        | Description             |
|------------|-------------------|-------------------------|
| `mcp_bash` | command, timeout? | Execute shell commands  |

**Pre-approved patterns:** `ls`, `cat`, `grep`, `find`, `git`, `cargo`, `just`, `make`, `aws` (read-only), `gh`

### Linear Agent (+6 tools = 25 total)

| Tool                   | Parameters                          | Description           |
|------------------------|-------------------------------------|-----------------------|
| `linear_read_issue`    | issue (ID/identifier/URL)           | Get issue details     |
| `linear_search_issues` | query, filters?                     | Search issues         |
| `linear_create_issue`  | team, title, description?           | Create new issue      |
| `linear_archive_issue` | issue                               | Archive an issue      |
| `linear_add_comment`   | issue, body                         | Comment on issue      |
| `linear_get_metadata`  | type (users/teams/projects/states/labels) | Look up Linear metadata |

### Playwright Agent (+18 tools = 37 total)

| Tool                      | Parameters  | Description          |
|---------------------------|-------------|----------------------|
| `browser_navigate`        | url         | Go to URL            |
| `browser_navigate_back`   | -           | Browser back         |
| `browser_snapshot`        | -           | Accessibility tree   |
| `browser_take_screenshot` | -           | Capture screenshot   |
| `browser_click`           | element     | Click element        |
| `browser_type`            | element, text | Type into element  |
| `browser_fill_form`       | fields[]    | Fill multiple fields |
| `browser_hover`           | element     | Hover over element   |

---

## Level 2: Sub-Agent Tools

Each "primary session" also has sub agents! I didn't like the way Claude Code would get lost deciding "which sub agent to use", nor did I like sub agents having the ability to make edits. So my sub agents are defined into an 8-way matrix, that simplifies down to 2 enum params.

### `ask_agent` Tool Matrix

| Agent Type   | Location   | Tools Available                                              | Model  |
|--------------|------------|--------------------------------------------------------------|--------|
| **Locator**  | Codebase   | ls, grep, glob                                               | Haiku  |
| **Locator**  | Thoughts   | + thoughts_list_documents                                    | Haiku  |
| **Locator**  | References | + thoughts_list_references                                   | Haiku  |
| **Locator**  | Web        | web_search, web_fetch                                        | Haiku  |
| **Analyzer** | Codebase   | read, ls, grep, glob, todowrite                              | Sonnet |
| **Analyzer** | Thoughts   | read, ls, grep, glob, thoughts_list_documents                | Sonnet |
| **Analyzer** | References | read, ls, grep, glob, todowrite, thoughts_list_references    | Sonnet |
| **Analyzer** | Web        | All 7: web_search, web_fetch, read, grep, glob, ls, todowrite | Sonnet |

---

## GPT-5 Reasoner Integration

Over time, I found Anthropic models to not be up-to-snuff on "intelligence" or "reasoning". However, I both enjoy the tool calling performance and how it feels to talk to Opus far more than I have OpenAI models. Every Opus agent can reach out to GPT-5.2 xhigh with questions.

CLAUDE.md is auto-injected based on path, the query Opus sends is automatically optimized prior to being sent to GPT-5.2 xhigh, and there is an enum for whether the caller wants reasoning output or plan output. Prior to orchestration, this was the biggest lift in daily agent work.

> Two-phase prompt optimization tool for GPT-5: optimize metadata with Claude, then execute with GPT-5.

**Features:**
- **Two-phase architecture**: Optimizer (configurable model) analyzes file metadata, executor (GPT-5) processes full content
- **Directory support**: Automatically discover and include files from directories with filtering
- **Dual interfaces**: CLI and MCP (Model Context Protocol) support via universal-tool framework
- **Smart file handling**: Binary detection, UTF-8 validation, path normalization
- **Configurable traversal**: Control recursion, hidden files, extensions, and file limits
- **Type safety**: Strongly-typed Rust with comprehensive test coverage (40 tests)

### `ask_reasoning_model` Parameters

| Parameter       | Type              | Description                                                       |
|-----------------|-------------------|-------------------------------------------------------------------|
| `prompt`        | string            | The question or task                                              |
| `files`         | FileMeta[]        | {filename, description} pairs                                     |
| `directories`   | DirectoryMeta[]?  | {path, description, extensions?, recursive, include_hidden, max_files} |
| `prompt_type`   | enum              | `reasoning` or `plan`                                             |
| `output_filename` | string?         | Write to thoughts/ if provided                                    |

---

## Decision Flowchart

The vast majority of my work goes through a research → planning → implementation pipeline. There are of course some things that aren't caught until implementation, but I find the majority of work can be planned accurately prior to implementation, as long as context is managed well.

```
Need to do something?
        │
        ▼
┌──────────────────┐
│ Shell commands?   │──Yes──▶ bash / commit / describe_pr
└──────────────────┘
        │ No
        ▼
┌──────────────────┐
│ Linear issues?    │──Yes──▶ linear
└──────────────────┘
        │ No
        ▼
┌──────────────────┐
│ Browser needed?   │──Yes──▶ playwright
└──────────────────┘
        │ No
        ▼
┌──────────────────┐
│ Research/Plan?    │──Yes──▶ research / create_plan_init/final / implement_plan
└──────────────────┘
        │ No
        ▼
   Default session
```

---

## Code Review (/review)

- `/review` runs under the dedicated `ReviewClaude` agent.
- Review tools (`review_*`) are **not available** to the Normal agent.
- Workflow (fileless, cache-based):
  1. `review_diff_snapshot` generates a paginated git diff via pure git2, caches it server-side, and returns a `diff_handle`
  2. Four parallel `review_run(diff_handle, lens)` calls execute lens reviewers (security/correctness/maintainability/testing)
  3. Diff content is embedded directly in reviewer prompts (no `review.diff` file created)
  4. Findings are consolidated, deduped, and written as a thoughts artifact
  5. `just thoughts_sync` is executed

### Review Tools

| Tool                   | Description                                                      |
|------------------------|------------------------------------------------------------------|
| `review_diff_snapshot` | Generate paginated diff, cache server-side, return handle        |
| `review_run`           | Run lens-based review over cached diff (diff embedded in prompt) |
| `review_diff_page`     | Fetch specific page content by handle for dedupe/artifacts       |

### Tool boundaries

- Review orchestrator agent: may read files, run `just thoughts_sync`, call `review_*` tools, and write the artifact.
- Reviewer sub-agents: **Read + cli_ls/cli_grep/cli_glob only** (no git/bash/write/edit/just_execute).

---

## Generation Note

Every screenshot in this thread was generated with a single orchestrator prompt:

> "I want to build a map that basically shows all the top down tooling that happens in the whole pipeline. You can spawn an agent and ask what the list of tools it has access to, and do that for each of the commands that are labeled after a specific agent. So the 'top level' of the map I end up wanting is probably what list of tools you, yourself, have. Then we have the second layer, which are the different agents you'll see in the commands, then we have the sub agent layer, which you'll actually need to spawn a `/research` on and ask it to map out all of the sub agent strategies that are available and what tools they have. Let me know if you have questions along the way or get stuck with not being sure how to get any of the information you might be looking for."

No intervention ended up being required.

---

**Source:** https://x.com/allisonology/status/2031454667062829525
**Thread:** https://twitter-thread.com/t/2031454667062829525
