## Currently investigating:
- (none)

## Researched / Ready for planning:
- Web fetch & web search tools + Discord MCP integration. Research completed:
  - Research doc: `thoughts/completed/2026-01-10_to_2026-01-30_google_supported_schema/research/discord_mcp_web_tooling_infrastructure.md`
  - **Discord MCP** (config only, no code): Add glittercowboy/discord-mcp to opencode.json with dedicated agent. 3 meta-tools dispatch to 128 operations. Requires Python 3.12+, uv, bot token + guild ID.
  - **Web fetch tool** (new crate: `crates/tools/web-tools/`): Two composable backends + two output modes:
    - Static backend: `reqwest` + `htmd` for plain HTML (no JS rendering)
    - Rendered backend: Calls configurable external API (e.g. reader sidecar, crawl4ai) for JS-rendered pages
    - Raw markdown output (start here) and Haiku-wrapped output (build second, uses anthropic-async)
    - Content-type early check to short-circuit non-HTML (PDFs, JSON, images)
    - Strategy/trait design for backend+output composability
  - **Web search tool** (same crate): Regular MCP tool, just reqwest + API key. Provider TBD (Exa vs Serper — needs focused evaluation). Date injection in tool description (steal from OpenCode pattern).
  - **Subagent integration** (later): Replace Claude Code built-in WebSearch/WebFetch in `agent/config.rs` enabled_tools_for() Web locations with custom MCP tools.
  - OpenCode's implementations analyzed for reference (not the approach we want — plain fetch + Turndown, no JS rendering, no Haiku wrapping, Exa MCP with no visible auth).
  - References added: glittercowboy/discord-mcp, intergalacticalvariable/reader, unclecode/crawl4ai, allisoneer/web-reader, exa-labs/exa-mcp-server, letmutex/htmd (pre-existing)

## Deferred (pending SQLite migration):
- Token tracking with tiktoken instead of KB for thoughts files. Research completed:
  - tiktoken-rs already in workspace (used by gpt5-reasoner with o200k_base encoding)
  - `write_document()` is trivial: replace `content.len()` with `count_tokens(content)`
  - `list_documents()` is the problem: currently uses `meta.len()` (filesystem metadata). To get tokens, would need to read+tokenize every file (~80-300ms for 50 files vs ~5ms for metadata-only)
  - Sidecar caching approach considered but adds complexity that SQLite eliminates
  - DEFERRED: With SQLite, token count becomes a column computed on write, queried on list. No caching needed.
  - Research doc: `thoughts/completed/2026-01-10_to_2026-01-30_google_supported_schema/research/tiktoken_file_tracking_migration.md`

- agentic_logging integration for linear-tools and pr_comments. Research completed:
  - Only `linear-tools` needs logging from the linear family (linear-schema and linear-queries are libs without tool methods)
  - `pr_comments` also needs logging (3 tools) — NOW IMPLEMENTED with duplicated ToolLogCtx
  - Recommended pattern: simple function-call helper (not ToolLogCtx context pattern)
  - DEFERRED: agentic_logging's file-based primitives (LogWriter, JSONL, day-bucketing, fd-lock) will be obsolete once thoughts moves to SQLite. The ToolCallRecord schema survives but storage layer changes completely.
  - CLEANUP NEEDED: ToolLogCtx is duplicated in `coding-agent-tools/src/logging.rs` and `pr-comments/src/logging.rs`.
    When refactoring, move ToolLogCtx to agentic_logging with parameterized server name.
  - Research docs: `thoughts/completed/2026-01-10_to_2026-01-30_google_supported_schema/research/agentic_logging_integration_audit.md` and `agentic_logging_extraction_analysis.md`

## To plan/design:
- SQLite migration for thoughts. Current file-based structure (thoughts/{branch}/ with research/, plans/, artifacts/, logs/) would become database tables. Key questions:
  - Schema design: documents table, tool_calls table, branches table?
  - Sync strategy without git (SQLite replication? export/import?)
  - Config/storage unification across entire codebase
  - What happens to agentic_logging crate? Becomes thin wrapper over DB writes?
  - I'm also considering postgres. Some deeper thoughts around postgres vs sqlite and the full agent setup come to mind.

## To classify/investigate:
- Need to develop a configuration system, so everything can be configurable. Need heavy inspiration from opencode. E.g. similar "here is
  the schema" header stuff as their config and similar granularity of configurable options available. This absolutely needs to come
- I need to have a way to run higher-level orchestrator-style agents. I'm thinking like where I could just press tab in opencode to
swithc to an "orchestrator" agent that can then spawn an entire opencode agent with a command. We could even just start with only
supporting `research`. Where it can spawn the entire research loop adhoc, get the response, and then read the research document once
it's done. Our even better, we figure out how to make the setup look good as an enum of `research` `create_plan_init`,
`create_plan_final`, and `implement_plan`. We'll have to support resuming existing sessions to support create_plan_final. Maybe opencode
exposes some ID that we can use for resuming. Then we'll have to implement some type of automatic insertion of user message when
`implement_plan` (or others) are running, to be able to get like some summarized version written to an artifact or something of progress
made so far before the limit is up.
before any "big lifts" or "large changes". e.g. before database or before doing more agent work.
- Ambient git repo detection failures should be handled consistently across tool registries (TODO(2)):
  avoid empty owner/repo fallbacks; prefer clear, fast errors and consider a shared override mechanism.
- README.md could use a huge refresh. We'll be at the point where we can have all-inclusive instructions for setting up for any repo soon. Would be a lot better than just "Here is a list of tools" if we mentioned how they are used and what they are for and how to do the entire setup.
- Similar to the last one, a nice QoL would be to re-look at the brand-new thoughts setup experience. How can we make that more streamlined? We should probably enforce/require a primary "thoughts" repo, and have an initial setup command that actually populates it with everything it needs. Currently it initializes the old v1 config and that's just silly. That's not used anywhere anymore.
- a command for basically "Are you sure you're finished? What did you do or not do?" that can be run at the end of implement_plan. I find myself consistently asking this as a quick check. Using language like "reflect deeply on everything you did, did you cut any corners or make any changes to the plan?" etc. I also tend to need to say "Don't make any edits or anything, I just want to know the answer" to make sure claude doesn't get fix hungry.
- Weird bug where sometimes adding a reference to two different configs (different repos) and trying to run references sync fails. Is
annoying, may partially be resolved via the fix for org/repo directory structure, although not completely - Could be related to the
times where you try to run `thoughts references sync` and the clone command hands on the ssh auth (if it's https but still requires auth
for some reason, it happened when the repo moved or was incorrect). We need a soft failover for references that don't mount correctly.
Instead of hanging.
- just commands need to refresh when justfiles change. Right now they only populate based on when they first launch? Not sure of
specifics - It could also be caching is broken somehow? Do we have similar TTL caching in just check commands as we do in other tools?
- Pagination UX needs improvement across tools. pr_comments and just_execute both need pagination support, and the explicit pagination
instructions ("Showing X out of Y, call again for more") should be solved elegantly at the framework level rather than per-tool. The ls
tool has good phrasing but pr_comments only partially ported that style. Consider: should agentic-tools-utils pagination module provide
a standard output formatter? Or should pagination messaging be part of the MCP response schema itself?
- Investigate every single clippy allow and see if there is a better approach than manually defining a clippy allow
- Check to see if I'm setting server-specific timeouts for the various MCP servers of if the timeout is up to the client.

## Old (probably delete):
- universal tool could use a re-look at how useful the current CLI fucntionality actually is, and how much we have to re-implement with clap for the standard use cases we have.
- universal tool could potentially use an ability to modify things at runtime. There is potential to create strong dynamic tool params and types and such that we would need to use rmcp directly for currently.
- Update rust-toolchain to whatever latest stable is and fix all the things that pop up by upgrading to a new stable version - Is this
done?
- The agentic-mcp command line really should do some type of tooling seperation to make permissions handling easier. e.g. the tool names
  shouldn't be root levell. They should be similar to how we had them setup in the old system, with the prefixes in opencode.json
defined. I'm not sure what categories would be best? Maybe cli_ls for cli-type tools. Then the reasoning model request method I think
makes sense as `reasoning_model_request` (Kinda it's own standalone). Although I could see benefit in doing that in the same category as
the sub agents. like `ask_agent` and `ask_reasoning_model` could be the two there? So they are in the same "sub category". It's
important to both consider how I want to split tools for permissions as well as how the tokenizer will represent tool calls. e.g. I
don't want have them all start with the same token, but it could be worthwhile to have the same token(s) starting the tool name for
similar tools. This gives the model a stronger chance at calling the right tool instead of the wrong one at any given time. e.g.
`ask_blah` ends up tokenizing to `ask` and `_blah[1...]`. Therefore the tokenizer could choose ask as the token that it wants to choose,
and have the next token generated having a higher chance at success. I think the `just_search` and `just_execute` are similarly useful
in this regard.
