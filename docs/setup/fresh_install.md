# Fresh install (new machine)

## Supported platforms

- Linux and macOS are supported.
- Windows is not supported.

## System prerequisites

### Linux

- Git
- FUSE support
- `mergerfs`

### macOS (FUSE-T first)

Install **FUSE-T** (preferred), plus `unionfs-fuse`:

- FUSE-T (preferred)
- `unionfs-fuse`

**Alternative:** macFUSE (if you can't use FUSE-T), plus `unionfs-fuse`.

> If both FUSE-T and macFUSE are installed, the code prefers FUSE-T.

## Install the binaries

Choose one approach:

### A) Install from source (workspace)

```bash
cargo install --path apps/thoughts
cargo install --path apps/agentic-mcp
cargo install --path apps/opencode-orchestrator-mcp
cargo install --path apps/agentic
```

### B) Install via cargo-binstall (where supported)

- `thoughts-bin`
- `agentic-mcp`
- `opencode-orchestrator-mcp`

> Note: `agentic-bin` currently does not ship binstall metadata, so `cargo binstall` may not work for it.

### C) GitHub Releases

This repo publishes release artifacts; download the appropriate binary for your platform.

## OpenCode version pin (orchestrator)

`opencode-orchestrator-mcp` expects OpenCode **v1.3.17**. A mismatched OpenCode version will fail startup.

If you need launcher mode, the orchestrator supports:

- `OPENCODE_BINARY`
- `OPENCODE_BINARY_ARGS`

## Next steps

- If you are setting up an existing repository, continue with [`existing_repo.md`](./existing_repo.md).
- If you are adding Thoughts to a repository for the first time, continue with [`new_repo.md`](./new_repo.md).
- For config details, see [`../config.md`](../config.md).
- For auth details, see [`../auth.md`](../auth.md).

## MCP wiring examples (copy/paste)

### OpenCode (`opencode.json`) — exact block from this repo

```json
  "mcp": {
    "tools": {
      "type": "local",
      "command": [
        "agentic-mcp"
      ],
      "enabled": true
    },
    "orchestrator": {
      "type": "local",
      "command": [
        "opencode-orchestrator-mcp"
      ],
      "enabled": true
    },
    "playwright": {
      "type": "local",
      "command": [
        "npx",
        "@playwright/mcp@latest"
      ],
      "enabled": true
    }
  },
```

Key fields:

- `mcp.tools`: runs `agentic-mcp`.
- `mcp.orchestrator`: runs `opencode-orchestrator-mcp`.
- `type: "local"` with `command: [...]`: OpenCode's local MCP server shape.
- `enabled: true`: enables each configured server.

> Note: This block includes the trailing comma because the source block sits inside a larger JSON file.

### Minimal stdio MCP config (`.mcp.json`) — exact file example

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

Key fields:

- `mcpServers`: map of named servers.
- `type: "stdio"`: launches the MCP server as a subprocess over stdin/stdout.
- `command`: binary to execute.

### Claude Code permissions example (`.claude/settings.json`) — exact file example

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

Key fields:

- `permissions.allow`: explicitly allowed tool IDs.
- `permissions.deny`: explicitly denied built-ins and tools.

## Quick verification

- `agentic-mcp --list-tools` prints to **stderr**, so do not rely on stdout capture.
- In a git repo, `thoughts init` should succeed. Outside a git repo it errors with `Not in a git repository. Run 'git init' first.`
- If mounts disappear after reboot, use [`reboot.md`](./reboot.md), not `thoughts sync`.
