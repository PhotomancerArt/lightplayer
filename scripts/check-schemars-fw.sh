#!/usr/bin/env bash
# Guard against `schemars` leaking into firmware dependency graphs.
#
# JSON Schema generation (`lp-cli schema gen`, backed by the non-default
# `schema-gen` features on lpc-model/lpc-hardware) is host-only tooling.
# If `schemars` ever appears in an RV32 firmware graph, a schema-gen feature
# has been enabled somewhere it must not be (flash budget + no_std). This
# asserts `cargo tree -i schemars` is empty for the firmware packages, using
# the same package/target/feature combinations as the fw build recipes
# (justfile `build-fw-esp32` / `build-fw-emu`; `server` added to cover the
# largest fw-esp32 graph).
set -euo pipefail
cd "$(dirname "$0")/.."

RV32_TARGET="riscv32imac-unknown-none-elf"

fail=0

# check_graph <label> <dir> [cargo tree args...]
#
# `cargo tree -i schemars` exits non-zero with "did not match any packages"
# when schemars is absent from the graph — that is the PASS case. Exit 0
# means schemars IS in the graph (print the inverted tree and fail); any
# other error is surfaced as a hard failure, never swallowed.
check_graph() {
    local label="$1" dir="$2"
    shift 2
    local out status
    set +e
    out=$(cd "$dir" && cargo tree -i schemars --target "$RV32_TARGET" "$@" 2>&1)
    status=$?
    set -e
    if [ "$status" -eq 0 ]; then
        echo "schemars found in $label dependency graph:"
        echo "$out"
        fail=1
    elif ! grep -q "did not match any packages" <<<"$out"; then
        echo "cargo tree failed for $label:"
        echo "$out"
        fail=1
    fi
}

check_graph "fw-esp32 (esp32c6,server)" lp-fw/fw-esp32 --features esp32c6,server
check_graph "fw-emu" . -p fw-emu

if [ "$fail" -ne 0 ]; then
    echo
    echo "schemars must never reach firmware: schema generation is host-only"
    echo "tooling (lp-cli). Find the edge above and make the schema-gen"
    echo "feature (or the offending dependency) host-only again."
    exit 1
fi
echo "fw schemars graph check: OK"
