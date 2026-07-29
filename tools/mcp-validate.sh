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
# Pin the Inspector because its unversioned CLI grammar drifted and stopped
# recognizing stdio targets. In 0.18.0 the target immediately follows --cli.
inspector_package="${MCP_INSPECTOR_PACKAGE:-@modelcontextprotocol/inspector@0.18.0}"

set +e
inspector_help="$(npx -y "$inspector_package" --help 2>&1)"
inspector_help_code=$?
set -e
if [[ $inspector_help_code -ne 0 ]] || ! grep -q -- '--cli' <<<"$inspector_help"; then
	echo "MCP Inspector CLI smoke check failed for ${inspector_package}" >&2
	printf '%s\n' "$inspector_help" >&2
	exit 1
fi

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

	stderr_file="$(mktemp)"
	set +e
	output="$(npx -y "$inspector_package" --cli "$bin" --method tools/list 2>"$stderr_file")"
	code=$?
	set -e
	diagnostics="$(<"$stderr_file")"
	rm -f "$stderr_file"

	should_fail=0
	if [[ $code -ne 0 ]]; then
		should_fail=1
	fi
	if ! printf '%s' "$output" | node -e '
const fs = require("fs");
let value;
try {
  value = JSON.parse(fs.readFileSync(0, "utf8"));
} catch (_) {
  process.exit(1);
}
if (value === null || Array.isArray(value) || typeof value !== "object" || !Array.isArray(value.tools)) {
  process.exit(1);
}
'; then
		should_fail=1
	fi

	if [[ $should_fail -ne 0 ]]; then
		failures=$((failures + 1))
		failed_pkgs+=("$pkg")
		failed_codes+=("$code")
		failed_outputs+=("${diagnostics}${diagnostics:+$'\n'}${output}")
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
