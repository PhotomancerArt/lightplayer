#!/usr/bin/env bash
# The two-binary opt-level probe (M5 MD8 / DD6).
#
#   scripts/emu/two-binary-probe.sh <lp-emu-esp32c6-bin> <lp-cli-bin> [runs]
#
# Per-package `opt-level = 3` (root `Cargo.toml`, the D2 list) reaches only
# code CODEGEN'D in the listed crates. A generic instantiated in `lp-cli` — a
# crate that is NOT on the list, and so builds at the workspace's release
# `opt-level = "z"` — is codegen'd at "z" no matter what the emulator crate's
# override says. M6 lost 25 % to exactly that.
#
# So every execution phase measures the same workload through two real
# instantiations of the machine: the `lp-emu-esp32c6` binary (all opt-3) and
# `lp-cli emu run` (the same machine, instantiated from an opt-"z" crate).
# The two SPEEDUPS must agree; a gap wider than ~5 % is the symptom.
#
# Prints USER seconds per (binary, grade) on `render-basic`, best of `runs`,
# with the 1-minute load average beside each. Absolute times are not
# comparable between the two binaries (different link plumbing and console
# sinks); the ratio between an A run and a B run of the SAME binary is.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

emu_bin="${1:?usage: two-binary-probe.sh <lp-emu-esp32c6-bin> <lp-cli-bin> [runs]}"
cli_bin="${2:?usage: two-binary-probe.sh <lp-emu-esp32c6-bin> <lp-cli-bin> [runs]}"
runs="${3:-3}"

elf="${LP_EMU_C6_REF_RENDER_BASIC:-target/emu-ref/77384a894-render-basic/fw-esp32c6}"
[[ -f "$elf" ]] || { echo "two-binary-probe: no render-basic image at $elf" >&2; exit 1; }

out_dir="target/emu-bench/two-binary"
mkdir -p "$out_dir"

loadavg() { sysctl -n vm.loadavg | awk '{print $2}'; }

best_of() {
    # best_of <label> <runs> -- <command...>
    local label="$1" n="$2"; shift 3
    local best="" load="" t out
    out="$(mktemp)"
    for _ in $(seq "$n"); do
        local la; la="$(loadavg)"
        /usr/bin/time -p "$@" >"$out" 2>&1 || { echo "two-binary-probe: $label failed" >&2; tail -20 "$out" >&2; rm -f "$out"; exit 1; }
        t="$(awk '/^user /{print $2}' "$out")"
        if [[ -z "$best" ]] || awk "BEGIN{exit !($t < $best)}"; then best="$t"; load="$la"; fi
    done
    rm -f "$out"
    printf '%-28s %-4s %8s   load %s\n' "$label" "$grade" "$best" "$load"
}

echo "two-binary probe on render-basic, best of $runs runs, USER seconds"
echo "  emu bin: $emu_bin"
echo "  lp-cli : $cli_bin"
echo
for grade in t1 t2; do
    best_of "lp-emu-esp32c6" "$runs" -- \
        "$emu_bin" --elf "$elf" --timeout 8s --wall-timeout 600 \
        --exit-on '[render-loop] === DONE ===' \
        --uart0 "file:$out_dir/emu-$grade.uart" --time-grade "$grade"
    best_of "lp-cli emu run" "$runs" -- \
        "$cli_bin" emu run --elf "$elf" --timeout 8s --wall-timeout 600 \
        --exit-on '[render-loop] === DONE ===' \
        --link-kind uart0 --console "$out_dir/cli-$grade.console" --time-grade "$grade"
done
