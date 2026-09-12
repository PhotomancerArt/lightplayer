#!/usr/bin/env bash
# The PAIR: leg 2 of the emu-loop-redesign invariant, as a script.
#
#   scripts/emu/loop-identity.sh <before-binary> <after-binary> [out-dir]
#
# Two binaries, the `--interpreter` leg, `render-basic` at `t2` — the same
# protocol P4 §8b ran by hand and every phase of the loop milestone runs from
# here on. Five readings, sha256 each, per binary:
#
#   pin      `--pin-log` — every edge on every routed pad, in emission order
#   frames   `--dump-frames` — what the WS281x decoder read off the pad
#   uart     `--uart0` — what the firmware said
#   trap     `--trap-log` — every trap the hart TOOK (P1; see below)
#   trace    a second, 20 ms run with `--trace` — every MMIO access and every
#            interrupt-line transition
#
# # Why the interpreter leg, and why against the previous head
#
# `p6-oracle.sh` compares the two *cores of one binary* and says translated
# code is exact. It cannot say the interpreter's own loop did not move, because
# it is one of the two things being compared. The milestone's whole exactness
# argument rests on the interpreter's `run_until` being the unchanged reference
# (D2), so the second oracle is the previous merged head's binary running the
# same interpreter. A `DIFF` in any column here is a stop, not a note.
#
# # Branch-scratch, like `p6-oracle.sh`
#
# Not a product script and not CI's: it takes two paths a human chose, it
# hard-codes one image and one grade because that is the pair the milestone
# argues about, and `oracle-sweep.sh` is still the every-image sweep. It lives
# under `scripts/emu/` for the same reason `p6-oracle.sh` does — the next phase
# needs to run exactly what the last one ran.
#
# # A before-binary that predates `--trap-log`
#
# The trap log is P1's own addition, so on P1 itself the *before* binary does
# not have the flag. That column then reads `n/a` and says why, and the other
# four still have to be `same`. From P2 on both sides have it and `n/a` is a
# bug in the invocation, not a fact about the run.
set -euo pipefail

before="${1:?usage: loop-identity.sh <before-binary> <after-binary> [out-dir]}"
after="${2:?usage: loop-identity.sh <before-binary> <after-binary> [out-dir]}"
out="${3:-target/loop-identity}"

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

for bin in "$before" "$after"; do
    [[ -x "$bin" ]] || { echo "loop-identity: $bin is not an executable" >&2; exit 1; }
done

elf="target/emu-ref/77384a894-render-basic/fw-esp32c6"
if [[ ! -f "$elf" ]]; then
    echo "loop-identity: building the render-basic reference image" >&2
    scripts/emu/build-reference-image.sh \
        "esp32c6,server,radio,spike_uart0_link,memory_fs,bench_render_loop" 77384a894 none >&2
fi

mkdir -p "$out"

# Does this binary know `--trap-log`? Asked of `--help` rather than by running
# it and reading the error, so a binary that predates the flag costs nothing.
has_trap_log() { "$1" --help 2>/dev/null | grep -q -- '--trap-log'; }

# One binary, both windows.
legs() {
    # One assignment per line: `local a=$1 b=$a` expands every word before it
    # assigns any of them, so the second would read an unset `a` under `-u`.
    local bin="$1"
    local name="$2"
    local stem="$out/$name"
    local trap_args=()
    if has_trap_log "$bin"; then
        trap_args=(--trap-log "file:$stem.trap")
    fi
    # 500 ms, no trace: the fast paths are ON, which is the point.
    "$bin" --elf "$elf" --time-grade t2 --timeout 500ms --wall-timeout 1800 \
        --interpreter \
        --uart0 "file:$stem.uart" --dump-frames "file:$stem.jsonl" \
        --pin-log "file:$stem.pin" \
        ${trap_args[@]+"${trap_args[@]}"} \
        >"$stem.out" 2>"$stem.err"
    # 20 ms with the trace: every access and every line transition.
    "$bin" --elf "$elf" --time-grade t2 --timeout 20ms --wall-timeout 1800 \
        --interpreter \
        --uart0 "file:$stem-20ms.uart" --trace --trace-file "$stem.trace" \
        >"$stem-20ms.out" 2>"$stem-20ms.err"
}

echo "loop-identity: before = $before" >&2
legs "$before" before
echo "loop-identity: after  = $after" >&2
legs "$after" after

sha() { shasum -a 256 "$1" | cut -c1-16; }

fail=0
row() {
    local what="$1" ext="$2"
    local a="$out/before.$ext" b="$out/after.$ext"
    if [[ ! -f "$a" || ! -f "$b" ]]; then
        local why="one side did not produce it"
        if [[ "$ext" == "trap" ]]; then
            why="the before-binary predates --trap-log"
        fi
        printf '%-8s %18s %18s %8s  (%s)\n' "$what" \
            "$( [[ -f "$a" ]] && sha "$a" || echo -)" \
            "$( [[ -f "$b" ]] && sha "$b" || echo -)" "n/a" "$why"
        return
    fi
    local verdict=DIFF
    if cmp -s "$a" "$b"; then verdict=same; else fail=1; fi
    printf '%-8s %18s %18s %8s\n' "$what" "$(sha "$a")" "$(sha "$b")" "$verdict"
}

echo
printf '%-8s %18s %18s %8s\n' reading before after verdict
printf '%-8s %18s %18s %8s\n' -------- ------------------ ------------------ --------
row pin pin
row frames jsonl
row uart uart
row trap trap
row trace trace
echo
echo "stopped (before): $(grep -ah 'stopped after' "$out/before.err" | head -1)"
echo "stopped (after):  $(grep -ah 'stopped after' "$out/after.err" | head -1)"
echo "n: $(wc -l <"$out/after.pin" | tr -d ' ') pin edges, \
$(wc -l <"$out/after.jsonl" | tr -d ' ') frames, \
$(wc -l <"$out/after.trace" | tr -d ' ') trace lines\
$( [[ -f "$out/after.trap" ]] && echo ", $(wc -l <"$out/after.trap" | tr -d ' ') traps")"
echo "artefacts: $out/"

if (( fail )); then
    echo
    echo "loop-identity: NOT IDENTICAL — the interpreter's loop moved. This is a stop." >&2
    exit 1
fi
echo
echo "loop-identity: the pair is identical on every reading that both binaries produce."
