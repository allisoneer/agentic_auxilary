# Authentication

The auth model here is intentionally boring: `agentic.toml` stores non-secret config, while secrets live in environment variables. MCP clients launch `agentic-mcp` or `opencode-orchestrator-mcp` as subprocesses, so if you want those tools to see credentials, you inject the env vars into the subprocess config for that client.

## Auth matrix (implemented behavior)

| Service / Tooling | Primary env var | Alternative path(s) | Implementation location |
|---|---|---|---|
| GitHub (PR comments) | `GH_TOKEN` | `GITHUB_TOKEN`, then gh CLI stored auth state | `crates/tools/pr-comments/src/lib.rs` |
| Linear | `LINEAR_API_KEY` | Any bearer-style token passed via `LINEAR_API_KEY` | `crates/linear/tools/src/http.rs` |
| Exa | `EXA_API_KEY` | None found in tool usage | `crates/services/exa-async/src/config.rs` |
| OpenRouter (`gpt5_reasoner`) | `OPENROUTER_API_KEY` | None found | `crates/tools/gpt5-reasoner/src/client.rs` |
| Anthropic (SDK) | `ANTHROPIC_API_KEY` | `ANTHROPIC_AUTH_TOKEN`, or both headers together | `crates/services/anthropic-async/src/config.rs` |
| web-retrieval summarizer | `ANTHROPIC_API_KEY` | Fallback to Anthropic provider key discovered from OpenCode | `crates/tools/web-retrieval/src/haiku.rs` |

## Injecting env vars via MCP client config

### OpenCode local MCP server

OpenCode's local MCP server entries support an `environment` map. That map is applied to the spawned server process, which is exactly what you want for secrets.

```json
{
  "mcp": {
    "tools": {
      "type": "local",
      "command": ["agentic-mcp"],
      "environment": {
        "GH_TOKEN": "set-me-here",
        "EXA_API_KEY": "set-me-here",
        "OPENROUTER_API_KEY": "set-me-here"
      },
      "enabled": true
    }
  }
}
```

The important bit is the `environment` object, not the exact set of keys above. Put the credentials there for whichever tools you actually enabled.

## Claude Code state still matters for Claude-backed paths

OpenCode is the primary day-to-day path in this repo, but Claude Code auth state is still relevant when a workflow uses Claude-backed sub-agents or when you intentionally use the Claude Code orchestration path instead.

- Make sure Claude Code is installed and logged in on the same machine for those flows.
- Keep the existing env-var injection pattern above for tool credentials; Claude Code login state is separate from those MCP subprocess env vars.

Claude Code can pass env vars either per MCP server or at the session level. The important bit is still that the spawned MCP process receives the secrets it needs.

**Server-scoped env (only for this MCP server):**

```json
{
  "mcpServers": {
    "tools": {
      "type": "stdio",
      "command": "agentic-mcp",
      "env": {
        "GH_TOKEN": "set-me-here",
        "OPENROUTER_API_KEY": "set-me-here",
        "EXA_API_KEY": "set-me-here"
      }
    }
  }
}
```

**Session-scoped env (top-level `env` for the whole Claude Code session):**

```json
{
  "env": {
    "EXA_API_KEY": "set-me-here"
  }
}
```

Use `mcpServers.<name>.env` when only one MCP server should see a secret, and top-level `env` when the whole Claude Code session should inherit it.

## Notes

- `agentic.toml` intentionally does not store API keys.
- `pr_comments` checks `GH_TOKEN` before `GITHUB_TOKEN`, then falls back to gh CLI auth state.
- `gpt5_reasoner` currently uses OpenRouter-only auth and hardcodes the OpenRouter base URL.
- OpenCode keeps its own login/account state; this repo mostly consumes that state rather than replacing it.
- Claude-backed sub-agents still depend on local Claude Code auth state when that path is selected.

If you are still in initial setup mode, go back to [`./setup/README.md`](./setup/README.md). If auth looks fine but tools still fail, the next stop is [`./troubleshooting.md`](./troubleshooting.md).
