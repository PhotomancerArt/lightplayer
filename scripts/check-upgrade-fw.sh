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
set -euo pipefail
cd "$(dirname "$0")/.."

RV32_TARGET="riscv32imac-unknown-none-elf"

fail=0

# check_graph <label> <dir> [cargo tree args...]
#
# `cargo tree -i lpa-upgrade` exits non-zero with "did not match any packages"
# when the crate is absent from the graph — that is the PASS case. Exit 0
# means it IS in the graph (print the inverted tree and fail); any other error
# is surfaced as a hard failure, never swallowed.
check_graph() {
    local label="$1" dir="$2"
    shift 2
    local out status
    set +e
    out=$(cd "$dir" && cargo tree -i lpa-upgrade --target "$RV32_TARGET" "$@" 2>&1)
    status=$?
    set -e
    if [ "$status" -eq 0 ]; then
        echo "lpa-upgrade found in $label dependency graph:"
        echo "$out"
        fail=1
    elif ! grep -q "did not match any packages" <<<"$out"; then
        echo "cargo tree failed for $label:"
        echo "$out"
        fail=1
    fi
}

check_graph "fw-esp32c6 (esp32c6,server)" lp-fw/fw-esp32c6 --features esp32c6,server
check_graph "fw-emu" . -p fw-emu

if [ "$fail" -ne 0 ]; then
    echo
    echo "lpa-upgrade must never reach firmware: the device refuses an old"
    echo "project format, it does not migrate one. Find the edge above and"
    echo "move the upgrade call into the host/studio side (lpa-*, lp-cli)."
    exit 1
fi
echo "fw lpa-upgrade graph check: OK"
