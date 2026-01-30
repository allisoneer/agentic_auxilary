#!/bin/bash
# Script to run cargo commands silently but still show success/failure

set -euo pipefail

COMMAND=$1
shift

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Run the command
if $COMMAND "$@"; then
    echo -e "${GREEN}✓${NC} $COMMAND succeeded"
    exit 0
else
    echo -e "${RED}✗${NC} $COMMAND failed"
    exit 1
fi