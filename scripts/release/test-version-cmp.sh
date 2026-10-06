#!/usr/bin/env bash
# Tests for scripts/release/version-cmp.sh — the order that decides which
# release GitHub marks Latest. Run: scripts/release/test-version-cmp.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/release/version-cmp.sh
source "${here}/version-cmp.sh"

failures=0

expect_lt() {
    if ! release_version_lt "$1" "$2"; then
        echo "FAIL: expected $1 < $2" >&2
        failures=$((failures + 1))
    fi
    if release_version_lt "$2" "$1"; then
        echo "FAIL: expected NOT $2 < $1" >&2
        failures=$((failures + 1))
    fi
}

expect_refused() {
    local status=0
    release_version_lt "$1" "2026.10.05-1" 2>/dev/null || status=$?
    if [[ "${status}" -ne 2 ]]; then
        echo "FAIL: expected '$1' to be refused (exit 2), got ${status}" >&2
        failures=$((failures + 1))
    fi
}

# The number after the dash is a number, not a string.
expect_lt 2026.10.05-9 2026.10.05-10
expect_lt 2026.10.05-10 2026.10.06-1
expect_lt 2026.10.05-2 2026.10.05-11
# Each date field outranks the counter.
expect_lt 2026.09.30-40 2026.10.01-1
expect_lt 2025.12.31-99 2026.01.01-1
# Leading zeros are decimal (08 and 09 are not octal).
expect_lt 2026.08.09-1 2026.09.08-1

# Equal is not less.
if release_version_lt 2026.10.05-3 2026.10.05-3; then
    echo "FAIL: equal versions compared less" >&2
    failures=$((failures + 1))
fi

# Dev versions and stray tags never sort.
expect_refused abc1234
expect_refused abc1234-dirty-101500PT
expect_refused v2026.10.05-3
expect_refused 2026.10.5-3
expect_refused 2026.10.05

got="$(release_version_max 2026.10.05-9 2026.10.06-1 2026.10.05-10)"
if [[ "${got}" != "2026.10.06-1" ]]; then
    echo "FAIL: max picked ${got}, not 2026.10.06-1" >&2
    failures=$((failures + 1))
fi
if release_version_max 2026.10.05-9 abc1234 >/dev/null 2>&1; then
    echo "FAIL: max accepted a dev version" >&2
    failures=$((failures + 1))
fi

# The CLI form agrees with the function.
"${here}/version-cmp.sh" lt 2026.10.05-9 2026.10.05-10
if "${here}/version-cmp.sh" lt 2026.10.05-10 2026.10.05-9; then
    echo "FAIL: CLI lt" >&2
    failures=$((failures + 1))
fi

if [[ "${failures}" -ne 0 ]]; then
    echo "version-cmp: ${failures} failure(s)" >&2
    exit 1
fi
echo "version-cmp: ok"
