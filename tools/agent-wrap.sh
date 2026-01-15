#!/usr/bin/env bash
# agent-wrap.sh - Minimal output wrapper for just commands
# OUTPUT_MODE controls behavior:
# - minimal: capture output, print ✓/✗ + smart tail on failure
# - normal: pass-through (full output)
# - verbose: pass-through (extra verbosity handled by justfile args)

set -euo pipefail

OUTPUT_MODE="${OUTPUT_MODE:-minimal}"

task_name() {
  # Heuristic task name from argv
  if [[ $# -eq 0 ]]; then
    echo "task"
    return
  fi

  if [[ "$1" == "cargo" ]]; then
    # cargo subcommands
    if [[ $# -ge 2 ]]; then
      case "$2" in
        nextest)
          if [[ $# -ge 3 && "$3" == "run" ]]; then
            echo "test"
            return
          fi
          ;;
        test)   echo "test"; return ;;
        build)  echo "build"; return ;;
        check)  echo "check"; return ;;
        clippy) echo "clippy"; return ;;
        fmt)
          # If -- --check appears, call it fmt-check
          if printf '%q ' "$@" | grep -q -- " -- --check"; then
            echo "fmt-check"
          else
            echo "fmt"
          fi
          return
          ;;
        *) echo "$2"; return ;;
      esac
    fi
  fi

  # Fallback: basename of first arg
  basename "$1"
}

smart_tail() {
  local output="$1"
  local max_lines=60
  local default_lines=30

  local total_lines
  total_lines=$(echo "$output" | wc -l | tr -d ' ')

  if [[ "$total_lines" -le "$max_lines" ]]; then
    echo "$output"
    echo
    echo "[Showing all $total_lines lines]"
    return
  fi

  local tail_output
  tail_output=$(echo "$output" | tail -n "$default_lines")

  local first_line
  first_line=$(echo "$tail_output" | head -1 || true)
  if [[ "${first_line:-}" =~ ^[[:space:]] ]]; then
    local extra_lines=10
    tail_output=$(echo "$output" | tail -n $((default_lines + extra_lines)))

    mapfile -t lines_array <<< "$tail_output"
    local start_idx=0
    for ((i=0; i<${#lines_array[@]}; i++)); do
      if [[ ! "${lines_array[$i]}" =~ ^[[:space:]] ]] && [[ -n "${lines_array[$i]}" ]]; then
        start_idx=$i
      fi
      if [[ $i -ge $extra_lines ]]; then
        break
      fi
    done

    tail_output=""
    for ((i=start_idx; i<${#lines_array[@]}; i++)); do
      tail_output+="${lines_array[$i]}"$'\n'
    done
    tail_output=${tail_output%$'\n'}
  fi

  local shown_lines
  shown_lines=$(echo "$tail_output" | wc -l | tr -d ' ')

  echo "$tail_output"
  echo
  echo "[Showing last $shown_lines lines of $total_lines total]"
}

TASK_NAME="$(task_name "$@")"

case "${OUTPUT_MODE}" in
  minimal)
    TMP="$(mktemp)"
    if "$@" >"$TMP" 2>&1; then
      echo "✓ ${TASK_NAME}"
      rm -f "$TMP"
      exit 0
    else
      code=$?
      echo "✗ ${TASK_NAME} failed (exit $code)"
      echo
      smart_tail "$(cat "$TMP")"
      rm -f "$TMP"
      exit $code
    fi
    ;;

  normal|verbose)
    exec "$@"
    ;;

  *)
    echo "Unknown OUTPUT_MODE: ${OUTPUT_MODE}" >&2
    exit 1
    ;;
esac
