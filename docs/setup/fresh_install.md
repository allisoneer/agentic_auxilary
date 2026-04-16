# Fresh install (new machine)

This is the from-zero path: machine first, repo second. Linux and macOS are supported here; Windows is not.

## Install the OS-level tools first

Do these in this order. It saves a little backtracking later.

1. Install **bun** with your preferred method. I treat this as base machine setup because OpenCode depends on it.
2. Install **rustup** if you want `cargo install` / `cargo binstall`, then set the default toolchain:

   ```bash
   rustup default stable
   ```

3. Optionally install **cargo-binstall**. It is not required, just nicer for the binaries that publish binstall metadata.
4. Install the platform-specific mount tooling:

   - **Linux:** install `mergerfs`, make sure FUSE support is enabled, and keep Git available. Allison's own notes call out mergerfs `2.41.1`; if Ubuntu makes this annoying, check [issue #2](https://github.com/allisoneer/agentic_auxilary/issues/2).
   - **macOS:** install **FUSE-T** first if you can:

     ```bash
     brew install macos-fuse-t/homebrew-cask/fuse-t
     ```

     `unionfs-fuse` is also required on macOS. If you already use **macFUSE**, that works too; when both are present, the runtime prefers FUSE-T.

5. Install **OpenCode** at the version this repo currently expects:

   ```bash
   bun install -g opencode-ai@1.3.17
   ```

6. Install **Claude Code** with whatever install method you already trust. The official docs are here: <https://code.claude.com/docs/en/authentication.md>.

## Install the Rust binaries

There are four binaries in the usual setup story here: `thoughts`, `agentic-mcp`, `opencode-orchestrator-mcp`, and `agentic`.

### `cargo install` works for all four

```bash
cargo install --path apps/thoughts
cargo install --path apps/agentic-mcp
cargo install --path apps/opencode-orchestrator-mcp
cargo install --path apps/agentic
```

### `cargo binstall` works where metadata exists

```bash
cargo binstall thoughts-bin
cargo binstall agentic-mcp
cargo binstall opencode-orchestrator-mcp
```

`agentic-bin` does not currently publish binstall metadata, so use `cargo install` or a release artifact for that one.

### GitHub Releases are also valid

If you prefer prebuilt binaries, this repo publishes release artifacts. Grab the right one for your platform and put it somewhere on `PATH`.

## Log into Claude Code and OpenCode

Do this before you start blaming MCP wiring.

- **Claude Code:** launch `claude` and complete auth with your preferred method. The documented first-run path opens a browser login and can fall back to a pasted code flow. On macOS, credentials live in Keychain; on Linux they live in `~/.claude/.credentials.json`.
- **OpenCode:** launch `opencode` and make sure the providers you plan to use are authenticated there. OpenCode keeps config under `~/.config/opencode/`; provider auth state lives in its `auth.json` data file.

## OpenCode version pin (orchestrator)

`opencode-orchestrator-mcp` validates against OpenCode **v1.3.17**. If you point it at another version, startup can fail.

You usually do not need extra env vars here. `OPENCODE_BINARY` and `OPENCODE_BINARY_ARGS` are only for cases where the default pinned binary path or the plain `opencode` on `PATH` is not the one you want.

## MCP wiring examples

This is the part people usually want to copy-paste, so it belongs before the cheerful "you're done" section.

### OpenCode (`opencode.json`)

This is the same `mcp` block shape used in this repo, just lifted as a standalone snippet so it pastes cleanly:

```json
{
  "mcp": {
    "tools": {
      "type": "local",
      "command": ["agentic-mcp"],
      "enabled": true
    },
    "orchestrator": {
      "type": "local",
      "command": ["opencode-orchestrator-mcp"],
      "enabled": true
    },
    "playwright": {
      "type": "local",
      "command": ["npx", "@playwright/mcp@latest"],
      "enabled": true
    }
  }
}
```

`mcp.tools` runs `agentic-mcp`, `mcp.orchestrator` runs `opencode-orchestrator-mcp`, and `type: "local"` tells OpenCode to launch each one as a local subprocess.

### Minimal stdio MCP config (`.mcp.json`)

If you want the smallest possible stdio example, this repo also includes one:

```json
{
  "mcpServers": {
    "orchestrator": {
      "type": "stdio",
      "command": "opencode-orchestrator-mcp"
    }
  }
}
```

That is just a named server map plus the binary to execute. Good enough for sanity-checking a client before you layer on more config.

### Claude Code permissions example (`.claude/settings.json`)

This repo's Claude settings are intentionally narrow. The point is to allow only the orchestrator tools and deny a pile of built-ins.

```json
{
  "permissions": {
    "allow": [
      "mcp__orchestrator__run",
      "mcp__orchestrator__list_sessions",
      "mcp__orchestrator__list_commands",
      "mcp__orchestrator__get_session_state",
      "mcp__orchestrator__respond_permission",
      "mcp__orchestrator__respond_question"
    ],
    "deny": [
      "Bash",
      "Write",
      "Edit",
      "Glob",
      "Grep",
      "Agent",
      "Skill",
      "EnterWorktree",
      "ExitWorktree",
      "CronDelete",
      "CronList",
      "CronCreate",
      "ScheduleWakeup",
      "Monitor",
      "TaskStop",
      "WebSearch",
      "WebFetch",
      "NotebookEdit",
      "RemoteTrigger",
      "PushNotification",
      "ToolSearch",
      "TaskCreate",
      "TaskGet",
      "TaskList",
      "TaskOutput",
      "TaskUpdate",
      "EnterPlanMode",
      "ExitPlanMode",
      "AskUserQuestion"
    ]
  }
}
```

If you use Claude Code this way, the interesting bit is `permissions.allow`: it whitelists the orchestrator namespace explicitly instead of letting the session wander into everything else.

## Quick verification

- `agentic-mcp --list-tools` prints to **stderr**, so do not judge success by stdout alone.
- In a git repo, `thoughts init` should work. Outside one, it fails with `Not in a git repository. Run 'git init' first.` It only needs a git repo for that step, not an existing remote.
- If you later register an already-cloned repo by local path (for example `thoughts mount add /path/to/repo ...`), that local repo needs an `origin` remote so Thoughts can record the canonical repo mapping.
- If mounts vanish after a reboot, use [`./reboot.md`](./reboot.md). `thoughts sync` is not the remount command.

## Next steps

If the repo already has `.thoughts/config.json`, continue with [`./existing_repo.md`](./existing_repo.md). If you're adding Thoughts to a repo for the first time, continue with [`./new_repo.md`](./new_repo.md). When you need the file-level details, jump to [`../config.md`](../config.md) and [`../auth.md`](../auth.md).
