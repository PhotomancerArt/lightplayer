#!/usr/bin/env bash
# M7 P6 scratch: the free oracle in the shape #678 and #680 used, one cell at a
# time so no invocation outruns its timeout.
#
#   scripts/emu/p6-oracle.sh <out-dir> <image-slug> <grade> <window> [jit-flags...]
#
# Runs the SAME binary twice — `--jit <flags>` against `--interpreter` — on one
# image at one grade to one emulated bound, and diffs the four readings the
# oracle compares: the UART0 capture, the `stopped after` line, the frame dump
# (or the trace, at a 20 ms window), and stdout/stderr.
#
# Branch-scratch, not a product script: `oracle-sweep.sh` takes two BINARIES and
# this needs two FLAG SETS of one, which is the same deviation #678 declared.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

out="${1:?usage: p6-oracle.sh <out-dir> <slug> <grade> <window> [jit flags...]}"
slug="${2:?}"
grade="${3:?}"
window="${4:?}"
shift 4

bin="target/release/lp-emu-esp32c6"
elf="target/emu-ref/77384a894-$slug/fw-esp32c6"
mkdir -p "$out"

leg() {
    local name="$1"
    shift
    local stem="$out/$slug-$grade-$window-$name"
    local extra=()
    if [[ "$window" == "20ms" ]]; then
        extra=(--trace --trace-file "$stem.trace")
    fi
    "$bin" --elf "$elf" --time-grade "$grade" --wall-timeout 1800 --timeout "$window" \
        --uart0 "file:$stem.uart" --dump-frames "file:$stem.jsonl" \
        ${extra[@]+"${extra[@]}"} "$@" >"$stem.out" 2>"$stem.err"
}

leg jit --jit --jit-report "$@"
leg interp --interpreter

a="$out/$slug-$grade-$window-jit"
b="$out/$slug-$grade-$window-interp"
same() { if cmp -s "$1" "$2"; then echo same; else echo DIFFERENT; fi; }

# stderr differs on exactly one line by construction — the interpreter block
# cache's own hit/flush counters, which MUST differ because translated code
# serves entries the cache would have served. Everything else on it is masked
# by dropping that one line, and the raw diff is printed so it can be checked.
# `jit:` lines are masked too, and for a different reason: they exist only on
# the `--jit --jit-report` leg and say what was translated, which is not guest
# state at all. They are printed in full beside the table instead.
mask() { grep -v -e '^blocks: ' -e '^jit: ' "$1"; }
mask "$a.err" >"$a.err.masked"
mask "$b.err" >"$b.err.masked"

printf '%-16s %-4s %-7s %7s %8s %8s %8s %9s\n' image grade window uart stopped frames stdout "stderr*"
printf '%-16s %-4s %-7s %7s %8s %8s %8s %9s\n' "$slug" "$grade" "$window" \
    "$(same "$a.uart" "$b.uart")" \
    "$(if [[ "$(grep -h 'stopped after' "$a.out" "$a.err" | head -1)" == "$(grep -h 'stopped after' "$b.out" "$b.err" | head -1)" ]]; then echo same; else echo DIFFERENT; fi)" \
    "$(if [[ "$window" == "20ms" ]]; then same "$a.trace" "$b.trace"; else same "$a.jsonl" "$b.jsonl"; fi)" \
    "$(same "$a.out" "$b.out")" \
    "$(same "$a.err.masked" "$b.err.masked")"
echo "n: $(if [[ "$window" == "20ms" ]]; then wc -l <"$a.trace" | tr -d ' '; echo -n ' trace lines'; else wc -l <"$a.jsonl" | tr -d ' '; echo -n ' frames'; fi)"
echo "stopped: $(grep -h 'stopped after' "$a.out" "$a.err" | head -1)"
echo "--- the masked stderr line, raw ---"
diff <(grep '^blocks: ' "$a.err") <(grep '^blocks: ' "$b.err") || true
