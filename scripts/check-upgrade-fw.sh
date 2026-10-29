#!/usr/bin/env bash
# Guard against `lpa-upgrade` leaking into firmware dependency graphs.
#
# Project format upgrades are host/studio tooling: the device never migrates a
# project (ADR 2026-07-05, decision 5 — it refuses an old format and says so).
# If `lpa-upgrade` ever appears in an RV32 firmware graph, something has wired
# the migrator into the device path, costing flash for a decision that was
# made against. This asserts `cargo tree -i lpa-upgrade` is empty for the
# firmware packages, using the same package/target/feature combinations as the
# fw build recipes (justfile `build-fw-esp32c6` / `build-fw-emu`; `server`
# added to cover the largest fw-esp32c6 graph).
#
# Same shape as scripts/check-schemars-fw.sh — kept separate so each gate
# names its own reason, which is what a failure needs to explain.
#
# `lpa-update` is checked too: since the update's project stage (M3 P10) it
# depends on `lpa-upgrade`, and it is the host side of an update — a board
# runs `lpc-update`, never it.
set -euo pipefail
cd "$(dirname "$0")/.."

RV32_TARGET="riscv32imac-unknown-none-elf"

fail=0

# check_graph <crate> <label> <dir> [cargo tree args...]
#
# `cargo tree -i <crate>` exits non-zero with "did not match any packages"
# when the crate is absent from the graph — that is the PASS case. Exit 0
# means it IS in the graph (print the inverted tree and fail); any other error
# is surfaced as a hard failure, never swallowed.
check_graph() {
    local crate="$1" label="$2" dir="$3"
    shift 3
    local out status
    set +e
    out=$(cd "$dir" && cargo tree -i "$crate" --target "$RV32_TARGET" "$@" 2>&1)
    status=$?
    set -e
    if [ "$status" -eq 0 ]; then
        echo "$crate found in $label dependency graph:"
        echo "$out"
        fail=1
    elif ! grep -q "did not match any packages" <<<"$out"; then
        echo "cargo tree failed for $label:"
        echo "$out"
        fail=1
    fi
}

for crate in lpa-upgrade lpa-update; do
    check_graph "$crate" "fw-esp32c6 (esp32c6,server)" lp-fw/fw-esp32c6 --features esp32c6,server
    check_graph "$crate" "fw-emu" . -p fw-emu
done

if [ "$fail" -ne 0 ]; then
    echo
    echo "lpa-upgrade (and lpa-update, which links it) must never reach firmware:"
    echo "the device refuses an old"
    echo "project format, it does not migrate one. Find the edge above and"
    echo "move the upgrade call into the host/studio side (lpa-*, lp-cli)."
    exit 1
fi
echo "fw lpa-upgrade + lpa-update graph check: OK"
