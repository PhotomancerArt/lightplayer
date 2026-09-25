#!/usr/bin/env bash
# Re-record every pinned firmware figure for the named chips — or check them.
#
#   scripts/bless-chips.sh [--check] [target...]
#
# Targets, run in the order given and ONE AT A TIME (each builds firmware, and the
# desk is shared — never two chips' builds in parallel):
#
#   esp32c6   the C6 chip heap record + lp-emu/esp/figures/esp32c6.json
#   esp32v3   the classic's chip heap record + lp-emu/esp/figures/esp32v3.json
#   esp32s3   the S3's chip heap record + lp-emu/esp/figures/esp32s3.json
#   engine    the engine heap records (scripts/heap-budget-record/engine/, RV32
#             fw-emu; no chip firmware, but the same "a code change moved a
#             pinned number" churn, so the one command covers it)
#
# No target means all four.
#
# What a bless does per chip, both halves through the recipes that already own
# them — this script adds no measurement of its own:
#
#   1. `heap-budget-check.sh chips-baseline <chip>`: the chip's heap record,
#      rewritten only if its gated figures moved (docs/heap-budget-gate.md).
#   2. the chip's boot suite with LP_EMU_BLESS=1: every test that observes a
#      figure through `lp-emu-esp-figures` writes the value it saw into the
#      chip's record instead of failing. Everything else in those tests still
#      asserts, so a bless of a broken tree still fails — a bless can only
#      rewrite FIGURES, never an identity pin (silicon, transcripts, pinned
#      reference images, structural constants), because those are literals
#      the bless cannot reach.
#
# `--check` runs the same suites and heap gates WITHOUT the bless: what CI's
# jobs run, for the named chips, in one place.
#
# Afterwards it prints `git diff --stat` of every record it can write, so the
# moves are in front of you before you commit them. Commit them in the change
# that moved them, and say what moved them. If only the EMULATOR changed, a
# moved figure is a finding: do not bless it.
#
# See docs/chip-figures.md.
set -euo pipefail
cd "$(dirname "$0")/.."

ALL="esp32c6 esp32v3 esp32s3 engine"
check=0
targets=()
for arg in "$@"; do
    case "$arg" in
    --check) check=1 ;;
    -h | --help)
        sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    esp32c6 | esp32v3 | esp32s3 | engine) targets+=("$arg") ;;
    c6) targets+=(esp32c6) ;;
    v3 | esp32) targets+=(esp32v3) ;;
    s3) targets+=(esp32s3) ;;
    *)
        echo "bless-chips: unknown target '$arg' (known: $ALL, or --check)" >&2
        exit 2
        ;;
    esac
done
[ "${#targets[@]}" -gt 0 ] || read -r -a targets <<<"$ALL"

# A firmware build writes gigabytes under target/; on a shared desk a full disk
# is the failure that wastes the most time, so say so before starting.
free_gib="$(df -g . 2>/dev/null | awk 'NR==2 {print $4}' || true)"
if [ -n "$free_gib" ] && [ "$free_gib" -lt 100 ]; then
    echo "bless-chips: only ${free_gib} GiB free here; a firmware build wants more than 100." >&2
    echo "bless-chips: continuing, but check 'df -h' if a build dies with ENOSPC." >&2
fi

if [ "$check" = 1 ]; then
    export LP_EMU_BLESS=0
    verb="checking"
else
    export LP_EMU_BLESS=1
    verb="blessing"
fi

fail=()
run() {
    # One step; a failure is recorded and the next target still runs, so one
    # invocation reports every chip.
    local what="$1"
    shift
    echo
    echo "=== bless-chips: ${verb} ${what}: $*"
    if ! nice -n 10 "$@"; then
        fail+=("${what}")
    fi
}

for t in "${targets[@]}"; do
    case "$t" in
    esp32c6)
        # The C6's suite resolves (and builds) its own images through
        # `lp_emu_esp32c6::test_support` under LP_EMU_BUILD_FW=1 — the first
        # line of `test-emu-c6-boot`, without the replays behind it.
        if [ "$check" = 1 ]; then
            run "esp32c6 heap" just heap-budget-check-chips-c6
        else
            run "esp32c6 heap" just heap-budget-baseline-chips esp32c6
        fi
        run "esp32c6 figures" env LP_EMU_BUILD_FW=1 \
            cargo test -p lp-emu-esp32c6 -- --include-ignored
        ;;
    esp32v3)
        # The recipe builds the shipped, rmt-chase and frame-dump images one
        # after another and the pinned reference image, and names each by
        # path; the suite is the one CI's `Emulator ESP32v3 (x64)` runs.
        if [ "$check" = 1 ]; then
            run "esp32v3 heap" just heap-budget-check-chips-v3
        else
            run "esp32v3 heap" just heap-budget-baseline-chips-v3
        fi
        run "esp32v3 figures" just test-emu-esp32v3-boot
        ;;
    esp32s3)
        if [ "$check" = 1 ]; then
            run "esp32s3 heap" just heap-budget-check-chips-s3
        else
            run "esp32s3 heap" just heap-budget-baseline-chips-s3
        fi
        run "esp32s3 figures" just test-emu-esp32s3-boot
        ;;
    engine)
        if [ "$check" = 1 ]; then
            run "engine heap" just heap-budget-check
        else
            run "engine heap" just heap-budget-baseline
        fi
        ;;
    esac
done

echo
echo "=== bless-chips: records"
git --no-pager diff --stat -- lp-emu/esp/figures scripts/heap-budget-record || true
if [ "${#fail[@]}" -gt 0 ]; then
    echo "bless-chips: FAILED: ${fail[*]}" >&2
    if [ "$check" = 0 ]; then
        echo "bless-chips: a bless only rewrites figures; a failure above is a test that failed \
for another reason (or a firmware build that did not finish) and needs reading, not re-blessing." >&2
    fi
    exit 1
fi
if [ "$check" = 1 ]; then
    echo "bless-chips: every figure and heap record matches for: ${targets[*]}"
else
    echo "bless-chips: blessed ${targets[*]}. Review the diff above, then commit it with the \
change that moved it."
    if [ "${GITHUB_ACTIONS:-}" != "true" ]; then
        echo "bless-chips: POSITIONAL figures (a stack high-water, say) were not written: the \
record holds CI's value for those, and a desk build can read another. If CI then fails on one, \
its message carries the new value (docs/chip-figures.md)."
    fi
fi
