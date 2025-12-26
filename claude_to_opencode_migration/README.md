# Claude Code to OpenCode Migration Script

A self-contained Python CLI that migrates Claude Code configurations into OpenCode format. Uses `uv` with PEP 723 inline dependencies for zero-setup execution.

## Quick Start

```bash
# Dry-run to see what would change
uv run claude_to_opencode_migration/migrate_claude_to_opencode.py --dry-run

# Migrate everything
uv run claude_to_opencode_migration/migrate_claude_to_opencode.py --all

# Migrate specific components
uv run claude_to_opencode_migration/migrate_claude_to_opencode.py --agents
uv run claude_to_opencode_migration/migrate_claude_to_opencode.py --commands
uv run claude_to_opencode_migration/migrate_claude_to_opencode.py --permissions
uv run claude_to_opencode_migration/migrate_claude_to_opencode.py --mcp
```

## What It Migrates

| Source (Claude Code) | Target (OpenCode) |
|---------------------|-------------------|
| `.claude/agents/*.md` | `.opencode/agent/*.md` |
| `.claude/commands/*.md` | `.opencode/command/*.md` |
| `.claude/settings.json` | `opencode.json` (permission/tools) |
| `~/.claude.json` / `.mcp.json` | `opencode.json` (mcp) |

## Transformations

### Agents
- Removes `name` field, adds `mode: subagent`
- Converts `tools` string to tools object with `"*": false` pattern
- Maps model aliases: `sonnet` → `anthropic/claude-sonnet-4-5`
- Maps colors to hex: `yellow` → `#EAB308`
- Normalizes tool names: `mcp__tools__ls` → `tools_ls`

### Commands
- Adds `description` from first heading or filename
- Maps model if present

### Permissions
- `Bash(git log:*)` → `permission.bash["git log *"] = "allow"`
- `mcp__server__*` patterns → `tools["server_*"] = true`
- Safe defaults: `bash["*"] = "ask"`, core tools enabled

### MCP Servers
- `stdio` → `local` with command array
- `sse`/`http` → `remote` with url
- `env` → `environment`
- Adds `enabled: true`

## CLI Options

```
--root ROOT           Project root (default: .)
--agents              Migrate agents only
--commands            Migrate commands only
--permissions         Migrate permissions only
--mcp                 Migrate MCP servers only
--all                 Migrate all (default if no specific flags)
--include-local       Include .claude/settings.local.json
--mcp-target          Where to write MCP config: project|global (default: project)
--dry-run             Show diffs without writing
--conflict            Conflict handling: skip|overwrite|prompt (default: skip)
--no-color            Disable colored output
```

## Safety Features

- **Dry-run mode**: Preview all changes with unified diffs
- **Timestamped backups**: Stored in `.opencode-migrate-backup/YYYYMMDD-HHMMSS/`
- **Conflict strategies**: Skip (default), overwrite, or prompt
- **Warnings**: Unsupported tools and unknown colors logged

## Running Tests

```bash
# Run all tests
uv run pytest claude_to_opencode_migration/tests/ -v

# Run unit tests only
uv run pytest claude_to_opencode_migration/tests/test_migration.py -v -k "not Integration"

# Run integration tests only
uv run pytest claude_to_opencode_migration/tests/test_migration.py -v -k "Integration"
```

## Directory Structure

```
claude_to_opencode_migration/
├── migrate_claude_to_opencode.py  # Main script
├── README.md                       # This file
└── tests/
    ├── test_migration.py          # Test suite
    └── fixtures/
        └── claude_sample/         # Sample Claude config for testing
            └── .claude/
                ├── agents/
                ├── commands/
                └── settings.json
```

## Unsupported Tools

The following tools are dropped during migration (with warnings):
- `WebSearch` - No OpenCode equivalent
- `Task` - Use @mention or subagent instead

## Known Limitations

- Hook migration is not supported
- Bidirectional migration (OpenCode → Claude) is not supported
- No JSON schema validation against OpenCode schema
