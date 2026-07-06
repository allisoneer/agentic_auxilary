#!/usr/bin/env bash
set -euo pipefail

ci="${CI:-false}"
wrap_mode="${AGENTIC_WRAP:-auto}"

should_wrap() {
	case "$wrap_mode" in
	1|true|yes)
		return 0
		;;
	0|false|no)
		return 1
		;;
	auto|"")
		;;
	*)
		echo "agent-wrap: invalid AGENTIC_WRAP='$wrap_mode' (expected auto|0|1)" >&2
		return 1
		;;
	esac

	if [[ "$ci" == "true" || "$ci" == "1" ]]; then
		return 1
	fi

	[[ -t 1 ]]
}

smart_tail() {
	local file="$1"
	local max_lines="${AGENTIC_SMART_TAIL_LINES:-200}"
	local total

	total="$(wc -l <"$file" | tr -d ' ')"
	if [[ "$total" -le "$max_lines" ]]; then
		cat "$file"
		return
	fi

	tail -n "$max_lines" "$file"
	echo ""
	echo "[showing last ${max_lines} lines of ${total}]"
}

task_name="${AGENTIC_TASK_NAME:-$(basename "${1:-task}")}"

if should_wrap; then
	tmp="$(mktemp -t agentic-wrap.XXXXXX)"
	trap 'rm -f "$tmp"' EXIT

	set +e
	"$@" >"$tmp" 2>&1
	code=$?
	set -e

	if [[ $code -eq 0 ]]; then
		echo "✓ ${task_name}"
		exit 0
	fi

	echo "✗ ${task_name} failed (exit ${code})"
	echo ""
	smart_tail "$tmp"
	exit "$code"
else
	exec "$@"
fi
