#!/bin/bash
# Smart cargo wrapper: quiet when successful, verbose and stops on failure

set -euo pipefail

# Colors
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

# Command and args
COMMAND="${1:-}"
shift || true

# Temp file for output
TEMP_OUTPUT=$(mktemp)
trap "rm -f $TEMP_OUTPUT" EXIT

# Extract a friendly name from the arguments
get_target_name() {
    for arg in "$@"; do
        if [[ "$arg" == "--manifest-path" ]]; then
            continue
        elif [[ "$arg" == *"/Cargo.toml" ]]; then
            basename "$(dirname "$arg")"
            return
        elif [[ "$arg" == "-p" ]]; then
            continue
        elif [[ "$arg" != -* ]]; then
            echo "$arg"
            return
        fi
    done
    echo "workspace"
}

TARGET_NAME=$(get_target_name "$@")

# Run the cargo command and capture output
EXIT_CODE=0
cargo "$COMMAND" "$@" > "$TEMP_OUTPUT" 2>&1 || EXIT_CODE=$?

# Count important metrics
if [[ "$COMMAND" == "test" ]]; then
    TEST_PASSED=$(grep -E "test result: ok\. ([0-9]+) passed" "$TEMP_OUTPUT" | \
                  awk '{sum += $4} END {print sum}' || echo "0")
    TEST_FAILED=$(grep -E "test result:.*([0-9]+) failed" "$TEMP_OUTPUT" | \
                  awk -F'[^0-9]+' '{for(i=1;i<=NF;i++) if($i~/^[0-9]+$/ && $(i-1)~/failed/) sum+=$i} END {print sum+0}' || echo "0")
    # Count actual warning messages, not the summary lines
    WARNING_COUNT=$(grep "^warning:" "$TEMP_OUTPUT" | grep -v "generated.*warning" | wc -l || true)
    
    if [[ $EXIT_CODE -ne 0 || $TEST_FAILED -gt 0 ]]; then
        # Test failed - show full output and exit
        echo -e "${RED}✗ $TARGET_NAME tests failed${NC}"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        cat "$TEMP_OUTPUT"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo -e "${RED}Stopping due to test failure in $TARGET_NAME${NC}"
        exit 1
    elif [[ $WARNING_COUNT -gt 0 ]]; then
        # Tests passed but with warnings - treat as failure
        echo -e "${YELLOW}✗ $TARGET_NAME: $TEST_PASSED tests passed but $WARNING_COUNT warnings${NC}"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        cat "$TEMP_OUTPUT"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo -e "${YELLOW}Stopping due to warnings in $TARGET_NAME${NC}"
        exit 1
    else
        # Clean success
        echo -e "${GREEN}✓${NC}  $TARGET_NAME: $TEST_PASSED tests passed"
    fi
    
elif [[ "$COMMAND" == "clippy" ]]; then
    # Count actual warning messages, not the summary lines
    WARNING_COUNT=$(grep "^warning:" "$TEMP_OUTPUT" | grep -v "generated.*warning" | wc -l || true)
    ERROR_COUNT=$(grep -c "^error:" "$TEMP_OUTPUT" || true)
    
    if [[ $EXIT_CODE -ne 0 || $ERROR_COUNT -gt 0 ]]; then
        # Clippy errors - show full output and exit
        echo -e "${RED}✗ $TARGET_NAME clippy errors${NC}"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        cat "$TEMP_OUTPUT"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo -e "${RED}Stopping due to clippy errors in $TARGET_NAME${NC}"
        exit 1
    elif [[ $WARNING_COUNT -gt 0 ]]; then
        # Warnings are also failures - show full output and exit
        echo -e "${YELLOW}✗ $TARGET_NAME has $WARNING_COUNT clippy warnings${NC}"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        cat "$TEMP_OUTPUT"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo -e "${YELLOW}Stopping due to clippy warnings in $TARGET_NAME${NC}"
        exit 1
    else
        # Clean
        echo -e "${GREEN}✓${NC}  $TARGET_NAME: clean"
    fi
    
elif [[ "$COMMAND" == "build" ]]; then
    # Count actual warning messages, not the summary lines
    WARNING_COUNT=$(grep "^warning:" "$TEMP_OUTPUT" | grep -v "generated.*warning" | wc -l || true)
    
    if [[ $EXIT_CODE -ne 0 ]]; then
        # Build failed - show full output and exit
        echo -e "${RED}✗ $TARGET_NAME build failed${NC}"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        cat "$TEMP_OUTPUT"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo -e "${RED}Stopping due to build failure in $TARGET_NAME${NC}"
        exit 1
    elif [[ $WARNING_COUNT -gt 0 ]]; then
        # Built with warnings - treat as failure
        echo -e "${YELLOW}✗ $TARGET_NAME: built with $WARNING_COUNT warnings${NC}"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        cat "$TEMP_OUTPUT"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo -e "${YELLOW}Stopping due to build warnings in $TARGET_NAME${NC}"
        exit 1
    else
        # Clean build
        echo -e "${GREEN}✓${NC}  $TARGET_NAME: built successfully"
    fi
    
else
    # Generic command
    if [[ $EXIT_CODE -ne 0 ]]; then
        echo -e "${RED}✗ $TARGET_NAME: $COMMAND failed${NC}"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        cat "$TEMP_OUTPUT"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo -e "${RED}Stopping due to failure in $TARGET_NAME${NC}"
        exit 1
    else
        echo -e "${GREEN}✓${NC}  $TARGET_NAME: $COMMAND succeeded"
    fi
fi

exit 0