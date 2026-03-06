#!/usr/bin/env bash
set -euo pipefail

# Validate one or more MCP server binaries using the MCP Inspector CLI.
# Usage:
#   tools/mcp-validate.sh <cargo-package-name>...

if [[ $# -eq 0 ]]; then
	echo "No MCP servers specified; nothing to validate." >&2
	exit 0
fi

pkgs=("$@")
target_dir="${CARGO_TARGET_DIR:-target}"

# Build all servers in one invocation (per requirements)
build_args=()
for pkg in "${pkgs[@]}"; do
	build_args+=("-p" "$pkg")
done

echo "Building MCP servers: ${pkgs[*]}" >&2
cargo build --quiet "${build_args[@]}"

failures=0
failed_pkgs=()
failed_codes=()
failed_outputs=()

for pkg in "${pkgs[@]}"; do
	bin="${target_dir}/debug/${pkg}"

	if [[ ! -x "$bin" ]]; then
		failures=$((failures + 1))
		failed_pkgs+=("$pkg")
		failed_codes+=("127")
		failed_outputs+=("Binary not found or not executable: ${bin}")
		continue
	fi

	echo "Validating MCP schemas: ${pkg}" >&2

	set +e
	output="$(npx -y @modelcontextprotocol/inspector --cli --transport stdio --method tools/list "$bin" 2>&1)"
	code=$?
	set -e

	should_fail=0
	if [[ $code -ne 0 ]]; then
		should_fail=1
	fi
	if grep -q "Failed to list tools" <<<"$output"; then
		should_fail=1
	fi
	if grep -qi "cannot be used without" <<<"$output"; then
		should_fail=1
	fi

	if [[ $should_fail -ne 0 ]]; then
		failures=$((failures + 1))
		failed_pkgs+=("$pkg")
		failed_codes+=("$code")
		failed_outputs+=("$output")
	fi
done

if [[ $failures -ne 0 ]]; then
	echo "MCP schema validation FAILED for ${failures} server(s): ${failed_pkgs[*]}" >&2
	for i in "${!failed_pkgs[@]}"; do
		echo >&2
		echo "===== ${failed_pkgs[$i]} (exit ${failed_codes[$i]}) =====" >&2
		printf '%s\n' "${failed_outputs[$i]}" >&2
	done
	exit 1
fi

echo "MCP schema validation passed" >&2
