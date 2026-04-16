# Authentication

Most credentials are read from environment variables inside the tool and service crates. If you run tools through MCP, set env vars in the MCP client's server configuration so they are present in the spawned `agentic-mcp` or `opencode-orchestrator-mcp` subprocess environment.

## Auth matrix (implemented behavior)

| Service / Tooling | Primary env var | Alternative path(s) | Implementation location |
|---|---|---|---|
| GitHub (PR comments) | `GH_TOKEN` | `GITHUB_TOKEN`, then gh CLI stored auth state | `crates/tools/pr-comments/src/lib.rs` |
| Linear | `LINEAR_API_KEY` | Any bearer-style token passed via `LINEAR_API_KEY` | `crates/linear/tools/src/http.rs` |
| Exa | `EXA_API_KEY` | None found in tool usage | `crates/services/exa-async/src/config.rs` |
| OpenRouter (`gpt5_reasoner`) | `OPENROUTER_API_KEY` | None found | `crates/tools/gpt5-reasoner/src/client.rs` |
| Anthropic (SDK) | `ANTHROPIC_API_KEY` | `ANTHROPIC_AUTH_TOKEN`, or both headers together | `crates/services/anthropic-async/src/config.rs` |
| web-retrieval summarizer | `ANTHROPIC_API_KEY` | Fallback to Anthropic provider key discovered from OpenCode | `crates/tools/web-retrieval/src/haiku.rs` |

## Notes

- `agentic.toml` intentionally does not store API keys.
- `pr_comments` checks `GH_TOKEN` before `GITHUB_TOKEN`, then falls back to gh CLI auth state.
- `gpt5_reasoner` currently uses OpenRouter-only auth and hardcodes the OpenRouter base URL.
- Some auth behavior, such as Claude Code login and OpenCode account state, is handled by external CLIs that this repo launches or interoperates with.

## Related docs

- Setup flows: [`./setup/README.md`](./setup/README.md)
- Configuration reference: [`./config.md`](./config.md)
- Troubleshooting: [`./troubleshooting.md`](./troubleshooting.md)
