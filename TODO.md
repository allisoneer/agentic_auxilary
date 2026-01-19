Currently investigating:
- (none)

Deferred (pending SQLite migration):
- Token tracking with tiktoken instead of KB for thoughts files. Research completed:
  - tiktoken-rs already in workspace (used by gpt5-reasoner with o200k_base encoding)
  - `write_document()` is trivial: replace `content.len()` with `count_tokens(content)`
  - `list_documents()` is the problem: currently uses `meta.len()` (filesystem metadata). To get tokens, would need to read+tokenize every file (~80-300ms for 50 files vs ~5ms for metadata-only)
  - Sidecar caching approach considered but adds complexity that SQLite eliminates
  - DEFERRED: With SQLite, token count becomes a column computed on write, queried on list. No caching needed.
  - Research doc: `thoughts/google_supported_schema/research/tiktoken_file_tracking_migration.md`

- agentic_logging integration for linear-tools and pr_comments. Research completed:
  - Only `linear-tools` needs logging from the linear family (linear-schema and linear-queries are libs without tool methods)
  - `pr_comments` also needs logging (3 tools)
  - Recommended pattern: simple function-call helper (not ToolLogCtx context pattern)
  - DEFERRED: agentic_logging's file-based primitives (LogWriter, JSONL, day-bucketing, fd-lock) will be obsolete once thoughts moves to SQLite. The ToolCallRecord schema survives but storage layer changes completely.
  - Research docs: `thoughts/google_supported_schema/research/agentic_logging_integration_audit.md` and `agentic_logging_extraction_analysis.md`

To plan/design:
- SQLite migration for thoughts. Current file-based structure (thoughts/{branch}/ with research/, plans/, artifacts/, logs/) would become database tables. Key questions:
  - Schema design: documents table, tool_calls table, branches table?
  - Sync strategy without git (SQLite replication? export/import?)
  - Config/storage unification across entire codebase
  - What happens to agentic_logging crate? Becomes thin wrapper over DB writes?

To classify/investigate:
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
- Update rust-toolchain to whatever latest stable is and fix all the things that pop up by upgrading to a new stable version - Is this
done?
- Investigate every single clippy allow and see if there is a better approach than manually defining a clippy allow
- Check to see if I'm setting server-specific timeouts for the various MCP servers of if the timeout is up to the client.
- I'd like to probably get to the point where I can add a web search/web fetch tool. I'm thinking probably integratino with exa for the
  web search. And then I need to investigate the best potential html->markdown tool. I really like how extensive jina.ai reader API is.
  I'm unsure if there are adequate local rust libraries that can do similar things as them, or if we'll want to rely on a provider. The
  other potential desire here is to look at maybe having something like claude code, where the "Web fetch" is actually a wrapper around
  a claude haiku call? Where the description for the `WebSearch` tool is like:
```
- Fetches content from a specified URL and processes it using an AI model
- Takes a URL and a prompt as input
- Fetches the URL content, converts HTML to markdown
- Processes the content with the prompt using a small, fast model
- Returns the model's response about the content
- Use this tool when you need to retrieve and analyze web content
```
And it uses haiku with a default prompt to translate by default, or has the optional prompt field for the agent to ask specific
questions. For both of these tools, we likely want to go and find references for what opencode and claude code both do for these tools.
Not because we want to copy 1:1, but because we likely want to be inspired by what they do. The output schema of claude code's web fetch
is:
```
{
type:
"object"

properties:
{
bytes:
{
description:
"Size of the fetched content in bytes"

type:
"number"

}
code:
{
description:
"HTTP response code"

type:
"number"

}
codeText:
{
description:
"HTTP response code text"

type:
"string"

}
result:
{
description:
"Processed result from applying the prompt to the content"

type:
"string"

}
durationMs:
{
description:
"Time taken to fetch and process the content"

type:
"number"

}
url:
{
description:
"The URL that was fetched"

type:
"string"

}
}
required:
[
0:
"bytes"

1:
"code"

2:
"codeText"

3:
"result"

4:
"durationMs"

5:
"url"

]
$schema:
"https://json-schema.org/draft/2020-12/schema"

additionalProperties:
false

}
```
And we likely want to be inspired by that. Except we'll probably want "tokens" instead of bytes. And probably some other niceties. We
can see how we use web fetch/web search currently in the `spawn_agent` tools. We use internal claude code tools for those, and the goal
will likely be to replace those with our own, and bring them into our whole ecosystem.
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


Old (probably delete):
- universal tool could use a re-look at how useful the current CLI fucntionality actually is, and how much we have to re-implement with clap for the standard use cases we have.
- universal tool could potentially use an ability to modify things at runtime. There is potential to create strong dynamic tool params and types and such that we would need to use rmcp directly for currently.
