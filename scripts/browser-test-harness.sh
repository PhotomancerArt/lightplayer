#!/usr/bin/env bash
# The cargo runner for the browser wasm suites: it runs
# `wasm-bindgen-test-runner` and then tells apart the two outcomes the runner
# itself reports identically.
#
# `wasm-bindgen-test-runner` ends every unhappy path with the same line:
#
#     Error: some tests failed
#
# and exit status 1 — including the path where no test ever finished because
# the browser died. That is not hypothetical. On PR #651 (CI run 34442317613
# attempt 1, job `Validate Browser (x64)`) headless Firefox's software
# compositor failed, geckodriver was SIGKILLed 23 s in, and the job reported
# "some tests failed" with no named failure and no verdict line. A re-run of
# the same commit passed in 2m45s. The same crash reproduces on macOS
# (2026-09-09, this worktree) with the identical signature:
#
#     [GFX1-]: RenderCompositorSWGL failed mapping default framebuffer, no dt
#     Failed to detect test as having been run. It might have timed out.
#     driver status: signal: 9 (SIGKILL)
#     Error: some tests failed
#
# From outside, an infrastructure crash was indistinguishable from a real
# regression, so the first thing anyone did was go hunting for a bug that was
# not there. This script makes the difference legible, and retries the crash
# once (loudly) because it is transient.
#
# WHAT COUNTS AS AN ENVIRONMENT FAILURE — deliberately narrow, because
# misclassifying a real regression as "infrastructure" is the worse error:
#
#   1. the driver process died (signal, or a non-zero exit of its own) AND the
#      suite never printed a `test result:` verdict, or
#   2. the driver never started at all (no WebDriver binary, bind failure).
#
# Everything else — every assertion failure, every panic, every wasm that
# refuses to load — is a test failure and is passed through untouched. Note
# what is NOT in the list: "no test output at all" on its own. A wasm that
# fails to link or a JS module that 404s also produces no test output, and
# both of those are real regressions this suite exists to catch (the runner's
# static server 404ing `browser_serial.js` is exactly how M2 found its
# serving-root bug). And "the suite ran and failed AND the driver was then
# killed" is a TEST failure: rule 1 requires the absence of a verdict, so a
# crash during teardown can never launder a red suite into a retry.
#
# Usage:
#   browser-test-harness.sh <test.wasm> [runner args...]   (as a cargo runner)
#   browser-test-harness.sh --self-test                    (check the rules)
#
# Env:
#   WASM_BROWSER_TEST_RETRIES  retries allowed for an environment failure
#                              (default 1, 0 disables). Never applies to a
#                              test failure.
set -euo pipefail

readonly PREFIX="browser test harness"

# ---------------------------------------------------------------------------
# Classification
# ---------------------------------------------------------------------------

# The runner draws progress with carriage returns and space padding, so its
# markers land mid-line ("...Waiting for test to finish...\r   driver status:
# signal: 9 (SIGKILL)"). Every match below is therefore substring, not
# anchored, and the log is CR-split first.
normalize_log() {
    tr '\r' '\n' < "$1"
}

# classify_run <exit-code> <log-file>
# Prints one line: "<verdict>\t<reason>" where verdict is pass|test|env.
classify_run() {
    local code="$1" log="$2" text
    if [ "$code" -eq 0 ]; then
        printf 'pass\t\n'
        return 0
    fi

    text="$(normalize_log "$log")"

    # A verdict line means the suite in the browser got far enough to count
    # its own results. Matched unanchored on purpose: when the runner times
    # out it re-prints the page's output div indented under "output div
    # contained:", and a verdict in THERE is still a verdict.
    local saw_verdict=1
    if ! grep -qE 'test result: (ok|FAILED)' <<<"$text"; then
        saw_verdict=0
    fi

    # The driver never came up: no verdict is possible, and nothing about the
    # code under test has been learned.
    local line
    if line="$(grep -m1 -E 'failed to find a suitable WebDriver binary|driver failed to start|driver failed to bind port during startup|failed to spawn (geckodriver|chromedriver|safaridriver|msedgedriver)' <<<"$text")"; then
        printf 'env\tthe browser driver never started: %s\n' "$(trim "$line")"
        return 0
    fi

    if [ "$saw_verdict" -eq 0 ]; then
        # The driver died under the suite.
        if line="$(grep -m1 -E 'driver status: signal:' <<<"$text")"; then
            printf 'env\tthe browser driver was killed (%s) before the suite reported a result\n' \
                "$(trim "${line#*driver status: }")"
            return 0
        fi
        if line="$(grep -m1 -E 'driver status: exit (status|code): [1-9]' <<<"$text")"; then
            printf 'env\tthe browser driver exited on its own (%s) before the suite reported a result\n' \
                "$(trim "${line#*driver status: }")"
            return 0
        fi
    fi

    printf 'test\t\n'
}

trim() {
    local s="$1"
    s="${s#"${s%%[![:space:]]*}"}"
    s="${s%"${s##*[![:space:]]}"}"
    printf '%s' "$s"
}

# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

banner() { printf '%s\n' "================================================================================" >&2; }

report_env_failure() {
    local reason="$1" log="$2" final="$3"
    banner
    {
        if [ "$final" = "final" ]; then
            echo "$PREFIX: ENVIRONMENT FAILURE, not a test failure."
        else
            echo "$PREFIX: ENVIRONMENT FAILURE, not a test failure — retrying once."
        fi
        echo "  $reason."
        echo
        echo "  The browser was taken away from the suite; the code under test never"
        echo "  got a verdict. Whatever the runner's own last line says above, no test"
        echo "  failed here, because no test finished. Do not go looking for a"
        echo "  regression on the strength of this run."
        if [ "$final" = "final" ]; then
            echo "  This PERSISTED across a retry, so it is not the usual transient crash:"
            echo "  suspect the machine, the browser install, or the driver."
        fi
        echo
        echo "  Runner and driver output tail (the runner's own capture):"
        normalize_log "$log" | grep -vE '^[[:space:]]*$' | tail -n 25 | sed 's/^/    | /'
    } >&2
    banner
}

# ---------------------------------------------------------------------------
# Run
# ---------------------------------------------------------------------------

run_suite() {
    local log="$1"
    shift
    local code=0
    # stderr folded into stdout so the captured log is in the order a reader
    # sees it, and `tee` so nothing is swallowed while we hold a copy.
    set +e
    "$@" 2>&1 | tee "$log"
    code="${PIPESTATUS[0]}"
    set -e
    return "$code"
}

main_run() {
    local retries="${WASM_BROWSER_TEST_RETRIES:-1}"
    local dir log code verdict reason line attempt=0
    if ! [[ "$retries" =~ ^[0-9]+$ ]]; then
        echo "$PREFIX: WASM_BROWSER_TEST_RETRIES must be a non-negative integer, got '$retries'" >&2
        return 2
    fi
    dir="$(mktemp -d "${TMPDIR:-/tmp}/browser-test-harness.XXXXXX")"
    # shellcheck disable=SC2064  # $dir must expand now, not at trap time
    trap "rm -rf '$dir'" EXIT
    log="$dir/run.log"

    while :; do
        code=0
        run_suite "$log" "$@" || code=$?

        line="$(classify_run "$code" "$log")"
        verdict="${line%%$'\t'*}"
        reason="${line#*$'\t'}"

        case "$verdict" in
            pass|test) return "$code" ;;
        esac

        if [ "$attempt" -ge "$retries" ]; then
            report_env_failure "$reason" "$log" "final"
            return "$code"
        fi

        attempt=$((attempt + 1))
        report_env_failure "$reason" "$log" "retrying"
        banner
        echo "$PREFIX: RETRYING (attempt $((attempt + 1)) of $((retries + 1))) — the crash above is transient." >&2
        echo "$PREFIX: a retry HAPPENED. If this run is green, the suite still cost a browser crash." >&2
        banner
    done
}

# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------

self_test() {
    local dir failures=0 total=0 fixture name expect code got verdict
    dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/testdata/browser-test-harness"
    if [ ! -d "$dir" ]; then
        echo "$PREFIX: self-test fixtures missing at $dir" >&2
        return 1
    fi

    shopt -s nullglob
    for fixture in "$dir"/*.log; do
        name="$(basename "$fixture" .log)"
        # Fixture name encodes the verdict it must produce, and with it the
        # exit code the runner paired with that output.
        case "$name" in
            pass-*) expect="pass"; code=0 ;;
            test-*) expect="test"; code=1 ;;
            env-*)  expect="env";  code=1 ;;
            *)
                echo "  ?? $name: fixture name must start with pass-, test- or env-" >&2
                failures=$((failures + 1))
                continue
                ;;
        esac
        total=$((total + 1))
        verdict="$(classify_run "$code" "$fixture")"
        got="${verdict%%$'\t'*}"
        if [ "$got" = "$expect" ]; then
            printf '  ok   %-34s -> %s\n' "$name" "$got"
        else
            printf '  FAIL %-34s -> %s (expected %s)\n' "$name" "$got" "$expect" >&2
            failures=$((failures + 1))
        fi
    done
    shopt -u nullglob

    if [ "$total" -eq 0 ]; then
        echo "$PREFIX: self-test found no fixtures in $dir" >&2
        return 1
    fi
    if [ "$failures" -ne 0 ]; then
        echo "$PREFIX: self-test FAILED ($failures of $total)" >&2
        return 1
    fi
    echo "$PREFIX: self-test ok ($total fixtures)"
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit $?
fi

if [ "$#" -eq 0 ]; then
    echo "$PREFIX: usage: $(basename "$0") <test.wasm> [runner args...] | --self-test" >&2
    exit 2
fi

main_run wasm-bindgen-test-runner "$@"
