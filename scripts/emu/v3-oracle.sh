#!/usr/bin/env bash
# The classic ESP32 (v3) machine's FREE ORACLE: the same binary, twice, with
# the fast path on and off, compared cell for cell.
#
#   scripts/emu/v3-oracle.sh <out-dir> <slug> <window> [extra flags...]
#   scripts/emu/v3-oracle.sh --bin-a <path> --bin-b <path> <out-dir> <slug> <window>
#
# `scripts/emu/p6-oracle.sh`'s classic twin. The C6's version asks `--jit`
# against `--interpreter`; this one asks the **block cache** against
# `--no-block-cache` today, and the flag pair is a parameter so P04's
# `--jit` / `--interpreter` slots straight in (`--fast "--jit" --slow
# "--interpreter"`).
#
# # What it is for
#
# The Xtensa speed ladder's invariant, in one command: *a run is a pure
# function of the instruction stream and its scripted input, and nothing the
# ladder lands changes a byte of what the classic machine already produces.*
# Every column must read `same`. A column that reads DIFFERENT is a stop, not
# a tolerance — see the plan's "The invariant".
#
# # The columns, and why each one is here
#
#   uart     UART0's bytes (`--uart0 file:`). The product's own transcript.
#   run      the `run: cycles=… instructions=… (core0=… core1=…)` line and the
#            `EXIT MATCHED cycle=` line. Cycles and per-core instruction counts
#            are the two numbers a cache hit could move and must not.
#   frames   the decoded WS281x frames off IO18 (`--dump-frames file:`) at a
#            long window; at a 20 ms window this cell is the `--trace` instead,
#            because 20 ms is before the first frame and a trace is the finer
#            reading anyway.
#   stdout   everything the run printed on stdout, which includes `run:`.
#   stderr*  everything on stderr with the `blocks:` lines dropped. Those MUST
#            differ — one leg has a cache and the other does not — and the raw
#            diff of exactly those lines is printed below the table so the
#            masking can be checked rather than trusted.
#
# # Pair mode
#
# `--bin-a` / `--bin-b` runs two BINARIES with the fast path **off** on both.
# That is the invariant's second leg: the merged `main` binary against the
# branch's, proving the interpreter itself did not move.
#
# ⚠️ **The two binaries need two flag strings, not one**, and that is the
# whole point of the leg rather than an inconvenience: the off-switch is what
# the branch ADDS, so the older binary does not have it and refuses it by
# name ("a door a later phase adds is absent rather than accepted-and-ignored").
# `--flags-a` / `--flags-b` name each leg's flags; each defaults to `--slow`.
# For P01 that is `--flags-a "" --flags-b "--no-block-cache"`: main has no
# cache to turn off, so its interpreter IS its default.
#
# # The images
#
# The three `scripts/emu/build-reference-image.sh --chip esp32` knows, by the
# slugs `bench-esp32v3.sh` uses: `boot-idle`, `shader-compile-stress`,
# `render-loop`. Both cores, `--core-quantum 256`, `--time-grade t1` — the
# only grade this machine has (plan decision E1).
#
# Emulated microseconds never gate anything (AGENTS.md): transcripts decide.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

bin_a=""
bin_b=""
flags_a=""
flags_b=""
have_flags_a=0
have_flags_b=0
fast_flags="${LP_EMU_V3_ORACLE_FAST:-}"
slow_flags="${LP_EMU_V3_ORACLE_SLOW:---no-block-cache}"
commit="${LP_EMU_V3_ORACLE_COMMIT:-0773c3fbd}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bin-a) bin_a="${2:?--bin-a needs a path}"; shift 2 ;;
        --bin-b) bin_b="${2:?--bin-b needs a path}"; shift 2 ;;
        --fast) fast_flags="${2?--fast needs a flag string}"; shift 2 ;;
        --slow) slow_flags="${2?--slow needs a flag string}"; shift 2 ;;
        --flags-a) flags_a="${2?--flags-a needs a flag string}"; have_flags_a=1; shift 2 ;;
        --flags-b) flags_b="${2?--flags-b needs a flag string}"; have_flags_b=1; shift 2 ;;
        -h|--help) sed -n '2,60p' "$0"; exit 0 ;;
        *) break ;;
    esac
done

out="${1:?usage: v3-oracle.sh [--bin-a P --bin-b P] <out-dir> <slug> <window> [extra flags...]}"
slug="${2:?}"
window="${3:?}"
shift 3

# The image, and the `--exit-on` sentinel that belongs to it. Kept identical
# to `bench-esp32v3.sh`'s table so the two scripts never measure two different
# things under one name.
case "$slug" in
    boot-idle)             exit_on="" ;;
    shader-compile-stress) exit_on="[inc-shader-compile] === DONE ===" ;;
    render-loop)           exit_on="[render-loop] === DONE ===" ;;
    *) echo "v3-oracle: unknown slug $slug (boot-idle|shader-compile-stress|render-loop)" >&2; exit 2 ;;
esac
elf="${LP_EMU_V3_ORACLE_ELF:-target/emu-ref/$commit-$slug/fw-esp32v3}"
[[ -f "$elf" ]] || {
    echo "v3-oracle: $elf is missing — scripts/emu/build-reference-image.sh --chip esp32 <features> $commit none" >&2
    exit 1
}

if [[ -n "$bin_a$bin_b" ]]; then
    [[ -n "$bin_a" && -n "$bin_b" ]] || { echo "v3-oracle: pair mode needs both --bin-a and --bin-b" >&2; exit 2; }
    mode=pair
    # The pair leg is about the INTERPRETER: both binaries run with the fast
    # path off, so what is compared is the thing neither of them may change.
    # Each leg names its own flags because the off-switch is what the branch
    # adds — see the header.
    [[ $have_flags_a == 1 ]] || flags_a="$slow_flags"
    [[ $have_flags_b == 1 ]] || flags_b="$slow_flags"
    a_bin="$bin_a"; a_flags="$flags_a"; a_name=main
    b_bin="$bin_b"; b_flags="$flags_b"; b_name=branch
else
    mode=flag
    a_bin="target/release/lp-emu-esp32v3"; a_flags="$fast_flags"; a_name=fast
    b_bin="$a_bin";                        b_flags="$slow_flags"; b_name=slow
fi
for b in "$a_bin" "$b_bin"; do
    [[ -x "$b" ]] || { echo "v3-oracle: $b is not an executable" >&2; exit 1; }
done

mkdir -p "$out"

leg() {
    local name="$1" runner="$2" flags="$3"
    local stem="$out/$slug-$window-$name"
    local extra=()
    # At 20 ms nothing has drawn a frame yet, so the fine reading is the trace;
    # at a long window it is the decoded frames off the pad.
    if [[ "$window" == "20ms" ]]; then
        extra=(--trace "$stem.trace")
    else
        extra=(--dump-frames "file:$stem.jsonl")
    fi
    if [[ -n "$exit_on" ]]; then
        extra+=(--exit-on "$exit_on")
    fi
    # Unquoted on purpose: `$flags` is a flag STRING the caller composed.
    # shellcheck disable=SC2086
    "$runner" --elf "$elf" --time-grade t1 --core-quantum 256 \
        --wall-timeout 1800 --timeout "$window" \
        --uart0 "file:$stem.uart" \
        ${extra[@]+"${extra[@]}"} $flags >"$stem.out" 2>"$stem.err"
}

leg "$a_name" "$a_bin" "$a_flags" "$@"
leg "$b_name" "$b_bin" "$b_flags" "$@"

a="$out/$slug-$window-$a_name"
b="$out/$slug-$window-$b_name"
same() { if cmp -s "$1" "$2"; then echo same; else echo DIFFERENT; fi; }

# The `blocks:` lines are the cache's own counters. They MUST differ between
# the two legs — that is what the two legs ARE — and they are not guest state:
# no transcript, no waveform and no cycle count can see them. Masked here and
# printed raw below.
mask() { grep -v -e '^blocks: ' "$1"; }
mask "$a.err" >"$a.err.masked"
mask "$b.err" >"$b.err.masked"

# The `run:` line plus the `EXIT MATCHED` line: cycles, per-core instructions,
# idle skips, unmapped counts, and the cycle the sentinel landed at.
runline() { grep -h -e '^run: ' -e '^EXIT MATCHED' "$1.out" "$1.err" || true; }
runline "$a" >"$a.run"
runline "$b" >"$b.run"

fine_label=frames
if [[ "$window" == "20ms" ]]; then fine_label=trace; fi

printf '%-22s %-8s %-7s %7s %7s %8s %8s %9s\n' \
    image window mode uart run "$fine_label" stdout "stderr*"
printf '%-22s %-8s %-7s %7s %7s %8s %8s %9s\n' \
    "$slug" "$window" "$mode" \
    "$(same "$a.uart" "$b.uart")" \
    "$(same "$a.run" "$b.run")" \
    "$(if [[ "$window" == "20ms" ]]; then same "$a.trace" "$b.trace"; else same "$a.jsonl" "$b.jsonl"; fi)" \
    "$(same "$a.out" "$b.out")" \
    "$(same "$a.err.masked" "$b.err.masked")"

if [[ "$window" == "20ms" ]]; then
    echo "n: $(wc -l <"$a.trace" | tr -d ' ') trace lines, $(wc -c <"$a.uart" | tr -d ' ') uart bytes"
else
    echo "n: $(wc -l <"$a.jsonl" 2>/dev/null | tr -d ' ') frames, $(wc -c <"$a.uart" | tr -d ' ') uart bytes"
fi
sed -n '1p' "$a.run"
echo "--- the masked lines, raw ($a_name then $b_name) ---"
grep -h '^blocks: ' "$a.err" || true
grep -h '^blocks: ' "$b.err" || true
