#!/usr/bin/env bash
# Unified cargo wrapper: OUTPUT_MODE-aware + zero-tolerance gates
set -euo pipefail

# Colors
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m'

# Mode: default minimal locally, normal in CI
DEFAULT_MODE="minimal"
if [[ "${CI:-false}" == "true" ]]; then
  DEFAULT_MODE="normal"
fi
MODE="${OUTPUT_MODE:-$DEFAULT_MODE}"

# Parse command and args
CMD="${1:-}"
shift || true
ARGS=("$@")

# Temp file for output capture
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

# Ensure RUSTFLAGS includes -Dwarnings for build/test
ensure_rustflags() {
  local rf="${RUSTFLAGS:-}"
  if [[ "$rf" != *"-Dwarnings"* ]]; then
    if [[ -z "$rf" ]]; then
      export RUSTFLAGS="-Dwarnings"
    else
      export RUSTFLAGS="$rf -Dwarnings"
    fi
  fi
}

# Build cargo command array
CARGO_CMD=("cargo")
case "$CMD" in
  fmt-check)
    CARGO_CMD+=("fmt")
    ;;
  clippy)
    CARGO_CMD+=("clippy")
    ;;
  test)
    CARGO_CMD+=("test")
    ensure_rustflags
    ;;
  build)
    CARGO_CMD+=("build")
    ensure_rustflags
    ;;
  *)
    CARGO_CMD+=("$CMD")
    ;;
esac

# Verbose mode adjustments
if [[ "$MODE" == "verbose" ]]; then
  case "$CMD" in
    clippy|build|test)
      ARGS=(--verbose "${ARGS[@]}")
      ;;
  esac
  if [[ "$CMD" == "test" ]]; then
    # Add --nocapture after -- delimiter
    has_delim="false"
    for i in "${!ARGS[@]}"; do
      if [[ "${ARGS[$i]}" == "--" ]]; then
        has_delim="true"
        # Insert --nocapture after the -- delimiter
        ARGS=("${ARGS[@]:0:$((i+1))}" "--nocapture" "${ARGS[@]:$((i+1))}")
        break
      fi
    done
    if [[ "$has_delim" == "false" ]]; then
      ARGS+=("--" "--nocapture")
    fi
  fi
fi

# Ensure fmt-check ends with '-- --check'
if [[ "$CMD" == "fmt-check" ]]; then
  need_check="true"
  for arg in "${ARGS[@]}"; do
    if [[ "$arg" == "--" ]]; then
      need_check="false"
      break
    fi
  done
  if [[ "$need_check" == "true" ]]; then
    ARGS+=("--" "--check")
  fi
fi

# Helpers
count_warnings() {
  grep "^warning:" "$TMP" | grep -v "generated.*warning" | wc -l | tr -d ' '
}

count_test_results() {
  local passed failed
  passed="$(grep -E "test result: ok\. ([0-9]+) passed" "$TMP" | awk '{sum += $4} END {print sum+0}' || echo "0")"
  failed="$(grep -E "test result:.*failed" "$TMP" | grep -oE "[0-9]+ failed" | awk '{sum += $1} END {print sum+0}' || echo "0")"
  echo "$passed:$failed"
}

run_and_capture() {
  if [[ "$MODE" == "minimal" ]]; then
    "${CARGO_CMD[@]}" "${ARGS[@]}" >"$TMP" 2>&1
  else
    "${CARGO_CMD[@]}" "${ARGS[@]}" 2>&1 | tee "$TMP"
  fi
}

# Execute
if run_and_capture; then
  case "$CMD" in
    test)
      IFS=':' read -r passed failed <<<"$(count_test_results)"
      warn_count="$(count_warnings || true)"
      if [[ "$warn_count" -gt 0 ]]; then
        echo -e "${YELLOW}✗ tests passed ($passed) but ${warn_count} warnings detected${NC}"
        [[ "$MODE" == "minimal" ]] && cat "$TMP"
        exit 1
      fi
      [[ "$MODE" == "minimal" ]] && echo -e "${GREEN}✓${NC} tests passed: ${passed}"
      [[ "$MODE" != "minimal" ]] && echo -e "${GREEN}Summary:${NC} tests passed: ${passed}, failed: ${failed}"
      ;;
    clippy)
      warn_count="$(count_warnings || true)"
      if [[ "$warn_count" -gt 0 ]]; then
        echo -e "${YELLOW}✗ clippy warnings: ${warn_count}${NC}"
        [[ "$MODE" == "minimal" ]] && cat "$TMP"
        exit 1
      fi
      [[ "$MODE" == "minimal" ]] && echo -e "${GREEN}✓${NC} clippy: clean"
      [[ "$MODE" != "minimal" ]] && echo -e "${GREEN}Summary:${NC} clippy clean"
      ;;
    build)
      warn_count="$(count_warnings || true)"
      if [[ "$warn_count" -gt 0 ]]; then
        echo -e "${YELLOW}✗ build produced ${warn_count} warnings${NC}"
        [[ "$MODE" == "minimal" ]] && cat "$TMP"
        exit 1
      fi
      [[ "$MODE" == "minimal" ]] && echo -e "${GREEN}✓${NC} build: success"
      [[ "$MODE" != "minimal" ]] && echo -e "${GREEN}Summary:${NC} build succeeded"
      ;;
    fmt-check)
      [[ "$MODE" == "minimal" ]] && echo -e "${GREEN}✓${NC} formatting: clean"
      [[ "$MODE" != "minimal" ]] && echo -e "${GREEN}Summary:${NC} formatting clean"
      ;;
    *)
      [[ "$MODE" == "minimal" ]] && echo -e "${GREEN}✓${NC} $CMD succeeded"
      [[ "$MODE" != "minimal" ]] && echo -e "${GREEN}Summary:${NC} $CMD succeeded"
      ;;
  esac
  exit 0
else
  echo -e "${RED}✗ $CMD failed${NC}"
  [[ "$MODE" == "minimal" ]] && cat "$TMP"
  exit 1
fi
