# Root-only Justfile for monorepo
# Per-crate justfiles have been removed; use crate-* commands instead

set shell := ["bash", "-euo", "pipefail", "-c"]

# CI/output mode detection

ci := env("CI", "false")
output_mode := env("OUTPUT_MODE", if ci == "true" { "normal" } else { "minimal" })

# Execution wrapper: only wrap in minimal mode

exec := if output_mode == "minimal" { "tools/agent-wrap.sh " } else { "" }

# Nextest args based on mode

nextest_args := if output_mode == "minimal" { "--status-level fail --failure-output immediate --hide-progress-bar" } else if output_mode == "verbose" { "--status-level all --verbose" } else { "" }

# Nextest profile based on mode and CI

nextest_profile := if output_mode == "minimal" { "minimal" } else if ci == "true" { "ci" } else { env("NEXTEST_PROFILE", "default") }

default: help

help:
    @echo "Workspace commands:"
    @echo "  just check            # fmt-check + clippy for entire workspace"
    @echo "  just test             # run tests for entire workspace"
    @echo "  just build            # build entire workspace"
    @echo "  just fmt              # format entire workspace"
    @echo "  just fmt-check        # check formatting for entire workspace"
    @echo ""
    @echo "Per-crate commands:"
    @echo "  just crate-check <c>  # check a single crate by name"
    @echo "  just crate-test <c>   # test a single crate by name"
    @echo "  just crate-build <c>  # build a single crate by name"
    @echo ""
    @echo "xtask commands:"
    @echo "  just xtask-sync       # sync autogen content (CLAUDE.md, release-plz.toml)"
    @echo "  just xtask-verify     # verify metadata, policy, and file freshness"
    @echo ""
    @echo "OUTPUT_MODE: minimal (local default) | normal (CI default) | verbose"

# Workspace-wide commands

check:
    {{ exec }}cargo fmt --all -- --check
    {{ exec }}cargo clippy --workspace --all-targets -- -D warnings

test:
    {{ exec }}cargo nextest run --workspace --profile {{ nextest_profile }} {{ nextest_args }}
    just mcp-test

build:
    {{ exec }}cargo build --workspace

fmt:
    {{ exec }}cargo fmt --all

fmt-check:
    {{ exec }}cargo fmt --all -- --check

# Check justfile formatting
fmt-check-just:
    @just --fmt --check --unstable

# Per-crate commands

crate-check name:
    {{ exec }}cargo fmt -p {{ name }} -- --check
    {{ exec }}cargo clippy -p {{ name }} --all-targets -- -D warnings

crate-test name:
    {{ exec }}cargo nextest run --profile {{ nextest_profile }} {{ nextest_args }} -E 'package({{ name }})'

crate-build name:
    {{ exec }}cargo build -p {{ name }}

# xtask commands

xtask-sync:
    {{ exec }}cargo run -p xtask -- sync

xtask-verify:
    {{ exec }}cargo run -p xtask -- verify

xtask-sync-check:
    {{ exec }}cargo run -p xtask -- sync --check

xtask-verify-check:
    {{ exec }}cargo run -p xtask -- verify --check

# Utility commands

thoughts_sync:
    {{ exec }}thoughts sync

# Copy a file
cp src dst:
    {{ exec }}cp "{{ src }}" "{{ dst }}"

# Create a directory (with parents)
mkdir path:
    {{ exec }}mkdir -p "{{ path }}"

# Set file executable
chmod-x path:
    chmod +x "{{ path }}"

# ------------------------------------------------------------------------------
# Git - Read-only Navigation (for agents without shell access)
#
# These recipes expose safe, read-only git inspection commands via just.
# All commands use --no-pager to avoid interactive hangs.
# ------------------------------------------------------------------------------

# git-context: Repo snapshot - root, branch/HEAD, remotes, status, last N commits. n defaults to 5.
git-context n="5":
    #!/usr/bin/env bash
    set -euo pipefail

    N="{{ n }}"
    if ! [[ "$N" =~ ^[0-9]+$ ]]; then
      echo "Invalid commit count '$N', defaulting to 5." >&2
      N=5
    fi

    if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      echo "Not inside a git repository." >&2
      exit 1
    fi

    echo "Repo root:"
    git rev-parse --show-toplevel
    echo

    current_branch="$(git symbolic-ref --short -q HEAD 2>/dev/null || true)"
    if [ -n "$current_branch" ]; then
      echo "Branch: $current_branch"
    else
      head_sha="$(git rev-parse --short HEAD 2>/dev/null || true)"
      if [ -n "$head_sha" ]; then
        echo "HEAD detached at $head_sha"
      else
        echo "No commits yet."
      fi
    fi
    echo

    echo "Remotes:"
    git --no-pager remote -v || true
    echo

    echo "Status (short):"
    git --no-pager status --porcelain -b || true
    echo

    echo "Recent commits:"
    git --no-pager log --decorate --oneline -n "$N" 2>/dev/null || echo "(no commits yet)"

# git-log: Commit history, optionally scoped to a path. n defaults to 20.
git-log n="20" path="":
    #!/usr/bin/env bash
    set -euo pipefail

    N="{{ n }}"
    if ! [[ "$N" =~ ^[0-9]+$ ]]; then
      echo "Invalid commit count '$N', defaulting to 20." >&2
      N=20
    fi

    P="{{ path }}"

    if [ -z "$P" ]; then
      git --no-pager log --decorate --graph --date=short -n "$N" \
        --pretty=format:'%C(auto)%h %ad %an%d %s' 2>/dev/null \
        || echo "(no commits yet)"
    else
      git --no-pager log --decorate --graph --date=short -n "$N" \
        --pretty=format:'%C(auto)%h %ad %an%d %s' -- "$P" 2>/dev/null \
        || echo "(no commits for path or path invalid)"
    fi

# git-diff: Diffs for staged/working tree. area: both|working|staged|head. format: stat|patch|name-only|name-status.
git-diff area="both" format="stat" path="":
    #!/usr/bin/env bash
    set -euo pipefail

    AREA="{{ area }}"
    FORMAT="{{ format }}"
    P="{{ path }}"

    case "$AREA" in
      both|working|staged|head) ;;
      *) echo "Invalid area: '$AREA'. Allowed: both|working|staged|head" >&2; exit 2 ;;
    esac

    fmt_flags=()
    case "$FORMAT" in
      stat)        fmt_flags+=(--stat) ;;
      patch)       fmt_flags+=(-p) ;;
      name-only)   fmt_flags+=(--name-only) ;;
      name-status) fmt_flags+=(--name-status) ;;
      *) echo "Invalid format: '$FORMAT'. Allowed: stat|patch|name-only|name-status" >&2; exit 2 ;;
    esac

    run_diff() {
      local cached="$1"
      if [ -z "$P" ]; then
        if [ "$cached" = "cached" ]; then
          git --no-pager diff --cached "${fmt_flags[@]}" || true
        else
          git --no-pager diff "${fmt_flags[@]}" || true
        fi
      else
        if [ "$cached" = "cached" ]; then
          git --no-pager diff --cached "${fmt_flags[@]}" -- "$P" || true
        else
          git --no-pager diff "${fmt_flags[@]}" -- "$P" || true
        fi
      fi
    }

    case "$AREA" in
      working)
        echo "=== Unstaged changes (working tree) ==="
        run_diff "working"
        ;;
      staged)
        echo "=== Staged changes (index) ==="
        run_diff "cached"
        ;;
      both)
        echo "=== Staged changes (index) ==="
        run_diff "cached"
        echo
        echo "=== Unstaged changes (working tree) ==="
        run_diff "working"
        ;;
      head)
        if [ -z "$P" ]; then
          git --no-pager diff "${fmt_flags[@]}" HEAD || true
        else
          git --no-pager diff "${fmt_flags[@]}" HEAD -- "$P" || true
        fi
        ;;
    esac

# git-blame: Annotate a file to see who last modified each line. Optional line range (start <= end).
git-blame file start="" end="":
    #!/usr/bin/env bash
    set -euo pipefail

    FILE="{{ file }}"
    START="{{ start }}"
    END="{{ end }}"

    if [ -z "$FILE" ]; then
      echo "Usage: just git-blame <file> [start] [end]" >&2
      exit 2
    fi

    range_args=()
    if [ -n "$START" ] || [ -n "$END" ]; then
      if ! [[ "$START" =~ ^[0-9]+$ ]] || ! [[ "$END" =~ ^[0-9]+$ ]]; then
        echo "Line range must be integers: start and end" >&2
        exit 2
      fi
      if [ "$START" -gt "$END" ]; then
        echo "Invalid range: start ($START) > end ($END)" >&2
        exit 2
      fi
      range_args=(-L "${START},${END}")
    fi

    git --no-pager blame -w "${range_args[@]}" -- "$FILE" || true

# git-show: Show commit details (ref only) or file contents at a ref (ref + path).
git-show ref path="":
    #!/usr/bin/env bash
    set -euo pipefail

    REF="{{ ref }}"
    P="{{ path }}"

    if [ -z "$REF" ]; then
      echo "Usage: just git-show <ref> [path]" >&2
      exit 2
    fi

    if [ -z "$P" ]; then
      git --no-pager show --stat --decorate --pretty=fuller "$REF" || true
    else
      git --no-pager show "${REF}:${P}" || true
    fi

# git-files: List tracked files, optionally filtered by pathspec patterns (quote paths with spaces).
git-files patterns="":
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -z "{{ patterns }}" ]; then
      git --no-pager ls-files
      exit 0
    fi

    git --no-pager ls-files -- {{ patterns }}

# ------------------------------------------------------------------------------
# MCP Inspector Recipes
# ------------------------------------------------------------------------------

# Interactive MCP Inspector for troubleshooting
# Usage:
#   just mcp-inspector              # default: tools/list method
#   just mcp-inspector resources/list
mcp-inspector method="tools/list":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p agentic-mcp
    BIN="./target/debug/agentic-mcp"
    if [ ! -x "$BIN" ]; then
      echo "agentic-mcp binary not found: $BIN" >&2
      exit 1
    fi
    echo "Launching MCP Inspector with method: {{ method }}"
    npx -y @modelcontextprotocol/inspector --cli --transport stdio --method "{{ method }}" "$BIN"

# CI-friendly MCP schema validation
# Builds agentic-mcp and validates with MCP Inspector, failing on any errors
mcp-test:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building agentic-mcp..."
    cargo build -p agentic-mcp
    BIN="./target/debug/agentic-mcp"
    
    echo "Validating schemas with MCP Inspector..."
    OUTPUT=$(npx -y @modelcontextprotocol/inspector --cli --transport stdio --method tools/list "$BIN" 2>&1) || true
    
    # Check for the fatal error pattern
    if echo "$OUTPUT" | grep -q "Failed to list tools"; then
      echo "MCP Inspector validation FAILED:"
      echo "$OUTPUT"
      exit 1
    fi
    
    # Check for schema errors (nullable without type, etc.)
    if echo "$OUTPUT" | grep -qi "cannot be used without"; then
      echo "MCP Inspector found schema errors:"
      echo "$OUTPUT"
      exit 1
    fi
    
    echo "MCP schema validation passed"
