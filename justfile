# Use bash with strict flags
set shell := ["bash", "-euo", "pipefail", "-c"]

# CI/output mode detection
ci := env("CI", "false")
output_mode := env("OUTPUT_MODE", if ci == "true" { "normal" } else { "minimal" })

# Tools to operate on (explicit; no autogeneration)
TOOLS := "anthropic_async claudecode_rs coding_agent_tools gpt5_reasoner linear_tools pr_comments thoughts_tool universal_tool"

default: help

help:
  @echo "Monorepo commands:"
  @echo "  just check          # run fmt-check + clippy for all tools"
  @echo "  just test           # run tests for all tools"
  @echo "  just build          # build all tools"
  @echo "  just fmt-all        # format all tools"
  @echo "  just fmt-check-all  # check formatting across all tools"
  @echo ""
  @echo "Per-tool examples:"
  @echo "  (cd thoughts_tool && just check)"
  @echo "  (cd universal_tool && just test)"
  @echo ""
  @echo "OUTPUT_MODE: minimal (local default) | normal (CI default) | verbose"

check:
  #!/usr/bin/env bash
  set -euo pipefail
  echo "━━━ Checking all tools ━━━"
  echo ""
  failures=0
  for tool in {{TOOLS}}; do
    echo "▶ Checking ${tool}..."
    if (cd "$tool" && OUTPUT_MODE="{{output_mode}}" just check >/dev/null 2>&1); then
      echo "  ✓ ${tool}: clean"
    else
      echo "  ✗ ${tool}: failed (run: cd ${tool} && just check)"
      failures=$((failures + 1))
    fi
  done
  echo ""
  if [ "$failures" -gt 0 ]; then
    echo "✗ ${failures} tool(s) failed checks"
    exit 1
  else
    echo "✓ All tools passed formatting and clippy checks"
  fi

test:
  #!/usr/bin/env bash
  set -euo pipefail
  echo "━━━ Testing all tools ━━━"
  echo ""
  failures=0
  for tool in {{TOOLS}}; do
    echo "▶ Testing ${tool}..."
    if (cd "$tool" && OUTPUT_MODE="{{output_mode}}" just test >/dev/null 2>&1); then
      echo "  ✓ ${tool}: tests passed"
    else
      echo "  ✗ ${tool}: tests failed (run: cd ${tool} && just test)"
      failures=$((failures + 1))
    fi
  done
  echo ""
  if [ "$failures" -gt 0 ]; then
    echo "✗ ${failures} tool(s) failed tests"
    exit 1
  else
    echo "✓ All tools passed tests"
  fi

build:
  #!/usr/bin/env bash
  set -euo pipefail
  echo "━━━ Building all tools ━━━"
  echo ""
  failures=0
  for tool in {{TOOLS}}; do
    echo "▶ Building ${tool}..."
    if (cd "$tool" && OUTPUT_MODE="{{output_mode}}" just build >/dev/null 2>&1); then
      echo "  ✓ ${tool}: built successfully"
    else
      echo "  ✗ ${tool}: build failed (run: cd ${tool} && just build)"
      failures=$((failures + 1))
    fi
  done
  echo ""
  if [ "$failures" -gt 0 ]; then
    echo "✗ ${failures} tool(s) failed to build"
    exit 1
  else
    echo "✓ All tools built successfully"
  fi

fmt-all:
  #!/usr/bin/env bash
  set -euo pipefail
  echo "━━━ Formatting all code ━━━"
  for tool in {{TOOLS}}; do
    echo "Formatting ${tool}..."
    (cd "$tool" && just fmt)
  done
  echo "✓ All code formatted"

fmt-check-all:
  #!/usr/bin/env bash
  set -euo pipefail
  echo "━━━ Checking formatting for all tools ━━━"
  failures=0
  for tool in {{TOOLS}}; do
    echo "▶ Checking ${tool} formatting..."
    if (cd "$tool" && OUTPUT_MODE="{{output_mode}}" just fmt-check >/dev/null 2>&1); then
      echo "  ✓ ${tool}: properly formatted"
    else
      echo "  ✗ ${tool}: formatting issues (run: cd ${tool} && just fmt)"
      failures=$((failures + 1))
    fi
  done
  echo ""
  if [ "$failures" -gt 0 ]; then
    echo "✗ ${failures} tool(s) have formatting issues"
    exit 1
  else
    echo "✓ All tools properly formatted"
  fi

thoughts_sync:
  thoughts sync
