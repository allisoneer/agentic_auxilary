#!/bin/bash

# Run a command silently, only showing output on failure
# Usage: ./run-silent.sh <command> [args...]

set -e

# Run the command, capturing output
if output=$("$@" 2>&1); then
    # Success - show green checkmark
    echo -e "\033[32m✓\033[0m $1"
    exit 0
else
    # Failure - show red X and output
    echo -e "\033[31m✗\033[0m $1"
    echo "$output"
    exit 1
fi