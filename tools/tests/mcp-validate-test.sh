#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture_dir="${repo_root}/tools/tests/fixtures"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
mkdir -p "$tmp_dir/bin" "$tmp_dir/target/debug"

cat >"$tmp_dir/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
pkg=""
while [[ $# -gt 0 ]]; do
	if [[ $1 == "-p" ]]; then
		pkg=$2
		shift 2
	else
		shift
	fi
done
: "${pkg:?mock cargo expected -p package}"
mkdir -p "${CARGO_TARGET_DIR}/debug"
printf '#!/usr/bin/env bash\nexit 0\n' >"${CARGO_TARGET_DIR}/debug/${pkg}"
chmod +x "${CARGO_TARGET_DIR}/debug/${pkg}"
EOF
chmod +x "$tmp_dir/bin/cargo"

cat >"$tmp_dir/bin/npx" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ " $* " == *" --help "* ]]; then
	printf 'usage: inspector --cli target\n'
	exit 0
fi
if [[ -n ${MCP_VALIDATE_FIXTURE_STDERR:-} ]]; then
	cat "$MCP_VALIDATE_FIXTURE_STDERR" >&2
fi
cat "${MCP_VALIDATE_FIXTURE:?fixture path required}"
exit "${MCP_VALIDATE_FIXTURE_EXIT_CODE:-0}"
EOF
chmod +x "$tmp_dir/bin/npx"

run_validation() {
	local fixture=$1
	local stderr_fixture=${2:-}
	PATH="$tmp_dir/bin:$PATH" \
		CARGO_TARGET_DIR="$tmp_dir/target" \
		MCP_VALIDATE_FIXTURE="$fixture" \
		MCP_VALIDATE_FIXTURE_STDERR="$stderr_fixture" \
		bash "$repo_root/tools/mcp-validate.sh" fixture-mcp
}

run_validation \
	"$fixture_dir/mcp-tools-valid.json" \
	"$fixture_dir/mcp-inspector-notice.txt"

if run_validation /dev/null "$fixture_dir/mcp-tools-deceptive.txt"; then
	echo "deceptive zero-exit diagnostics containing tools unexpectedly passed" >&2
	exit 1
fi

if run_validation "$fixture_dir/mcp-tools-object.json"; then
	echo "non-array top-level tools value unexpectedly passed" >&2
	exit 1
fi

echo "mcp-validate shell fixtures passed" >&2
