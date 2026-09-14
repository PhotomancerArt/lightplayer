#!/usr/bin/env bash
# **The classic ESP32's differential-oracle CI cell** (M7 XD14): the same
# emulator binary, the same pinned image, the same flags — `--jit` against
# `--interpreter` — and every reading compared byte for byte.
#
#   scripts/emu/xt-jit-identity-image.sh [slug] [window] [blocks] [out-dir]
#   scripts/emu/xt-jit-identity-image.sh boot-idle 100ms 12000 target/xt-jit-identity
#
# `jit-identity-image.sh` is the C6's; this is the classic's twin, and the
# three places it differs each have a reason below. `v3-oracle.sh` is the
# same idea with more columns and more images, for a branch and a desk;
# `bench-esp32v3.sh` owns the image table both of them resolve against.
#
# # What is compared
#
#   uart     the guest's UART0 bytes — what the firmware said
#   run      the `run: cycles=… instructions=… (core0=… core1=…)` line — what
#            the machine counted, per hart
#   frames   the WS281x frames decoded off IO18
#   stdout   the run report
#   stderr*  everything on stderr except two masked lines, below
#
# Two stderr lines are masked, each for a stated reason:
#
#   `blocks: …`  the INTERPRETER's block-cache counters. They MUST differ —
#                translated code serves the entries the cache would have
#                served — so the line is dropped and the raw diff printed
#                beside the table instead of being silently tolerated.
#   `jit: …`     exists only on the `--jit` leg and says what was translated
#                and what each event cost. Not guest state at all. Printed in
#                full underneath.
#
# # ⚠️ Difference 1: no `--trace` column, and why
#
# The C6's cell uses a 20 ms `--trace` window as its third reading. **The
# classic cannot**: `--trace` asks for the interpreter's own
# instruction-by-instruction reading, which a translated stay cannot emit, so
# a `--jit --trace` run installs **no core at all** and says so
# (`machine.rs`, and `tests/jit_default.rs::a_traced_run_refuses_the_core`).
# A cell built that way would compare a run against itself and pass. So this
# script REFUSES a `--trace`-sized window outright rather than quietly
# producing a green that means nothing, and the third reading is the decoded
# frames at a longer window. Left for the director as a ruling: XD14's "20 ms
# `--trace`" is the C6's shape and does not transfer.
#
# # ⚠️ Difference 2: the walk is bounded, and why
#
# The classic's three pinned images are all the same 9 MB firmware, so the
# whole-image walk is ~140,000 blocks and ~90 MB of wasm **whatever the slug**
# — 146 s of cranelift per core on an M2 Max, 293 s for the pair, against
# DD98's ~3-minute budget for this cell inside a 26-minute job. `--jit-blocks`
# bounds the walk; at 12,000 blocks the whole `--jit` leg is 26.5 s locally
# and both translation events still fire. The bound is a parameter so the
# director can move it against a real CI number rather than a projection.
#
# # ⚠️ Difference 3: it checks that BOTH events ran
#
# A cell that translated at boot and never met a publish would be half a cell.
# The `--jit` leg's report is checked for at least one entry into translated
# code AND at least one `publish-by-store` event, and the absence of either is
# a failure with a named reason.
#
# # Why this is never silent when it cannot run
#
# A binary built without `--features jit` still accepts `--jit` and then does
# nothing with it, which would make this script compare a run against itself
# and pass. The jit leg's stderr is checked for the translator's own coverage
# line, and its absence is a failure — never a skip.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

slug="${1:-boot-idle}"
window="${2:-100ms}"
blocks="${3:-12000}"
out="${4:-target/xt-jit-identity}"
grade=t1

bin="${LP_EMU_XT_JIT_IDENTITY_BIN:-target/release/lp-emu-esp32v3}"
commit="${LP_EMU_V3_ORACLE_COMMIT:-0773c3fbd}"

case "$slug" in
    boot-idle)             exit_on="" ;;
    shader-compile-stress) exit_on="[inc-shader-compile] === DONE ===" ;;
    render-loop)           exit_on="[render-loop] === DONE ===" ;;
    *) echo "xt-jit-identity-image: unknown slug $slug (boot-idle|shader-compile-stress|render-loop)" >&2; exit 2 ;;
esac

# See "Difference 1". A short window is exactly where `v3-oracle.sh` reaches
# for a `--trace`, and a `--trace` leg here would install no core.
if [[ "$window" =~ ^([0-9]+)ms$ ]] && (( BASH_REMATCH[1] <= 50 )); then
    echo "xt-jit-identity-image: $window is a --trace-sized window and the classic's --jit leg" >&2
    echo "  refuses the core under --trace, so the cell would compare a run against itself." >&2
    echo "  Use a window over 50 ms (the default is 100ms). See this script's header." >&2
    exit 2
fi

elf="${LP_EMU_V3_ORACLE_ELF:-target/emu-ref/$commit-$slug/fw-esp32v3}"
[[ -f "$elf" ]] || {
    echo "xt-jit-identity-image: $elf is missing." >&2
    echo "  Build it with: scripts/emu/build-reference-image.sh --chip esp32 <features> $commit none" >&2
    echo "  (or run it through \`just bench-esp32v3\`, which owns the feature table)" >&2
    exit 1
}
[[ -x "$bin" ]] || {
    echo "xt-jit-identity-image: $bin is not an executable." >&2
    echo "  Build it with: cargo build --release -p lp-emu-esp32v3 --features jit" >&2
    exit 1
}

mkdir -p "$out"
stem="$out/$slug-$grade-$window"

leg() {
    local name="$1"; shift
    local extra=()
    [[ -n "$exit_on" ]] && extra+=(--exit-on "$exit_on")
    # The leg's own stderr is where the emulator says why it would not start —
    # `--jit` is not even a recognised flag without `--features jit`, and under
    # `set -e` that would otherwise be a bare non-zero exit with nothing said.
    if ! "$bin" --elf "$elf" --time-grade "$grade" --core-quantum 256 \
        --wall-timeout 1800 --timeout "$window" \
        --uart0 "file:$stem-$name.uart" --dump-frames "file:$stem-$name.jsonl" \
        ${extra[@]+"${extra[@]}"} "$@" >"$stem-$name.out" 2>"$stem-$name.err"; then
        echo "xt-jit-identity-image: the $name leg exited non-zero. Its last lines:" >&2
        tail -10 "$stem-$name.err" >&2
        if [[ "$name" == jit ]]; then
            echo "  If that names --jit as unknown, this binary was built WITHOUT" >&2
            echo "  --features jit: cargo build --release -p lp-emu-esp32v3 --features jit" >&2
        fi
        exit 1
    fi
}

echo "xt-jit-identity-image: $slug $grade $window, walk bounded to $blocks block(s), through $bin"
echo "xt-jit-identity-image:   elf $elf ($(shasum -a 256 "$elf" | cut -c1-16))"
leg jit --jit --jit-report --jit-blocks "$blocks"
leg interp --interpreter

# The proof that the jit leg actually translated something. Without
# `--features jit` the flag is accepted and nothing is emitted, and then this
# script would be comparing a run against itself — green, and worth nothing.
if ! grep -aq '^jit: core0: ' "$stem-jit.err"; then
    echo "xt-jit-identity-image: the --jit leg printed no coverage line, so nothing was" >&2
    echo "  translated. This binary is almost certainly built WITHOUT --features jit." >&2
    echo "  That is a FAILURE, not a skip: a self-comparison would pass and prove nothing." >&2
    tail -5 "$stem-jit.err" >&2
    exit 1
fi
# Both events, or it is half a cell. See "Difference 3".
entries="$(sed -n 's/^jit: core0: \([0-9]*\) entries.*/\1/p' "$stem-jit.err" | head -1)"
publishes="$(grep -ac 'publish-by-store #' "$stem-jit.err" || true)"
if [[ -z "$entries" || "$entries" -eq 0 ]]; then
    echo "xt-jit-identity-image: the --jit leg made ZERO entries into translated code." >&2
    echo "  The boot event produced a core nothing ever entered, so this cell compares two" >&2
    echo "  interpreted runs. Check the refusal counts on the coverage line below." >&2
    grep -a '^jit: core0: ' "$stem-jit.err" >&2
    exit 1
fi
if (( publishes == 0 )); then
    echo "xt-jit-identity-image: the --jit leg saw NO publish-by-store event, so only one of" >&2
    echo "  the classic's two translation events (XD10) is covered. Use a longer window, or" >&2
    echo "  an image whose guest writes code." >&2
    exit 1
fi

mask() { grep -av -e '^blocks: ' -e '^jit: ' "$1"; }
mask "$stem-jit.err" >"$stem-jit.err.masked"
mask "$stem-interp.err" >"$stem-interp.err.masked"
runline() { grep -ha -e '^run: ' -e '^EXIT MATCHED' "$1.out" "$1.err" || true; }
runline "$stem-jit" >"$stem-jit.run"
runline "$stem-interp" >"$stem-interp.run"

fail=0
# `same` is called in a command substitution, which is a SUBSHELL — a `fail=1`
# set inside one is lost — so the verdict comes back as the word and the
# caller sets the flag.
same() { if cmp -s "$1" "$2"; then echo same; else echo DIFFER; fi; }
note() { [[ "$1" == same ]] || fail=1; }

v_uart="$(same "$stem-jit.uart" "$stem-interp.uart")"; note "$v_uart"
v_run="$(same "$stem-jit.run" "$stem-interp.run")"; note "$v_run"
v_frames="$(same "$stem-jit.jsonl" "$stem-interp.jsonl")"; note "$v_frames"
v_out="$(same "$stem-jit.out" "$stem-interp.out")"; note "$v_out"
v_err="$(same "$stem-jit.err.masked" "$stem-interp.err.masked")"; note "$v_err"

echo
printf '%-22s %-5s %-7s %8s %7s %8s %8s %9s\n' image grade window uart run frames stdout "stderr*"
printf '%-22s %-5s %-7s %8s %7s %8s %8s %9s\n' \
    "$slug" "$grade" "$window" "$v_uart" "$v_run" "$v_frames" "$v_out" "$v_err"
echo
echo "n: $(wc -l <"$stem-jit.jsonl" | tr -d ' ') frame(s), $(wc -c <"$stem-jit.uart" | tr -d ' ') uart bytes"
echo "events: boot + app-core release, and $publishes publish-by-store line(s); $entries entries on core 0"
sed -n '1p' "$stem-jit.run"
echo "--- the translated core's own lines (masked above, printed here) ---"
grep -a '^jit: ' "$stem-jit.err" || true
echo "--- the masked block-cache lines, raw (they MUST differ) ---"
diff <(grep -a '^blocks: ' "$stem-jit.err") <(grep -a '^blocks: ' "$stem-interp.err") || true

if (( fail )); then
    echo
    echo "xt-jit-identity-image: NOT IDENTICAL — translated and interpreted disagree. This is a stop." >&2
    exit 1
fi
echo
echo "xt-jit-identity-image: identical on all five readings."
