# Why this stack

If all you need is a thin MCP wrapper around an API, this repo is overbuilt for that purpose. That is not me being coy; it really is aimed at a different problem.

The point here is not "have some tools available over MCP." The point is to build a harness where orchestration, tool scoping, review boundaries, and repeatable workflows are part of the product instead of prompt folklore.

MCP matters because it keeps the pieces portable.

It is not the interesting part by itself.

## MCP is the transport, the harness is the product

Yes, the tools speak MCP. That part is deliberately portable.

The interesting part is everything built behind that interface: an orchestrator layer that manages long-lived OpenCode sessions, primary work agents with different permission and tool profiles, reasoning-model calls for deep passes, and an explicit locator/analyzer × codebase/thoughts/references/web sub-agent matrix instead of a vague "go spawn a helper if you feel like it" story.

That design shows up in a few concrete ways.

There is a dedicated orchestrator MCP server because session work is not just a single prompt/response round-trip; sessions can pause on permissions, surface questions, and be resumed later.

There is a unified `agentic-mcp` entry point because the repo is trying to expose a shaped tool surface, not a pile of unrelated binaries.

And there is `--allow` scoping because the single `agentic-mcp` binary is meant to be portable and highly configurable: the same entry point can expose a deliberately narrow tool surface for a given client or role.

The safety/control story is adjacent but separate: tools are grouped into explicit namespaces, and sub-agents run with strict MCP config plus preselected tool sets instead of inheriting a giant ambient surface.

This is also why the repo has an actual agent hierarchy instead of one mega-agent with everything attached.

There is orchestration-level control.

There are primary work agents.

There are reasoning-model calls that sit alongside the sub-agent system.

There are sub-agents that are split by role and location on purpose.

That is a harness decision, not a transport decision.

## Tool design here is mostly about removing bad choices up front

A lot of agent setups keep adding more tools and hope the model becomes wise enough to pick the good ones.

I do not think that scales especially well.

This repo goes the other direction: shape the tool surface so the bad steering paths are less available in the first place.

That is why review tooling lives in its own isolated namespace instead of being mixed into the normal coding flow.

It is why sub-agents are launched with strict MCP config and no permission-asking path, which forces the parent workflow to choose the right scoped helper ahead of time.

It is why normal work leans on `just` recipes and dedicated CLI tools rather than default bash access everywhere.

The repo absolutely can use shell access when it is the right tool.

It just does not assume shell should be the default answer to every problem.

That detail matters more than it sounds like it should.

If an agent can solve most routine work through structured tools and `just` recipes, you get repeatability and narrower failure modes almost for free.

The result is a stack that is a little more opinionated and a lot less likely to drift into random nonsense just because the model had too many doors open.

This is also the reason the repo does not really line up with the "just generate MCP tools from APIs" mindset.

Thin wrappers are easy to make.

The hard and useful part is deciding what an agent should be allowed to do, how it should discover information, when it should ask for help, and what work should be mechanically separated instead of prompt-separated.

That also shows up in tool shape: the repo prefers fewer tools, fewer required parameters, and low-ceremony interfaces so context goes to the task instead of the call surface. `cli_ls` doubles as both an `ls` and a tree via `depth`, some tools paginate implicitly when repeating the same request is the cleanest shape, and others use explicit `head_limit`/`offset` when a stateless scan is the better fit.

## The workflow is explicit: research, then plan, then implement

The command surface in this repo encodes an actual working loop.

Research commands produce grounded artifacts.

Planning commands resolve open questions and turn them into requirements plus an implementation plan.

Implementation commands then consume those artifacts and run through the work with verification gates instead of pretending the earlier context never mattered.

That same design philosophy shows up in the model split too.

There are distinct agent variants for Claude and GPT-5.4-oriented workflows.

The reasoner tool itself is two-phase: first do GPT-specific prompt optimization over the available context, then run the expensive reasoning pass.

Even the existence of a separate Bash agent follows the same pattern — shell access is available when it is truly the right tool, but it is not the default shape of every session.

The repo is trying to make good workflows easier to repeat.

Ad hoc prompting can still feel pretty magical here; the point is that explicit workflows are what make long-tail work repeatable instead of relying on a great one-off prompt.

So the short version is: this repo uses MCP everywhere, but it is not "just MCP." It is a constrained Rust agent stack built around session control, scoped tools, specialized sub-agents, structured execution, and an explicit research → plan → implement loop.

If you want the full map, including the agent hierarchy and tool matrices, read [`../workflow.md`](../workflow.md).
