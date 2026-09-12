#!/usr/bin/env bash
# **Tier (b) of the differential oracle** (M7 JD14, ruled by JD23): the same
# emulator binary, the same pinned image, the same flags — `--jit` against
# `--interpreter` — and every reading compared byte for byte.
#
#   scripts/emu/jit-identity-image.sh [slug] [grade] [window] [out-dir]
#   scripts/emu/jit-identity-image.sh harness t2 20ms target/jit-identity
#
# This is the CI gate. `oracle-sweep.sh` is the same idea across two BINARIES
# and the whole pinned set; `p6-oracle.sh` was the branch-scratch form every M7
# phase ran by hand, with the `77384a894` pin hardcoded in it. This one
# resolves the image by slug from the same table those two carry, so it works
# for every pinned image and not only the render pair.
#
# # What is compared
#
#   uart     the guest's UART0 bytes — what the firmware said
#   stopped  the `stopped after N cycles (U us, I instructions)` line — what
#            the machine counted
#   trace    the MMIO + interrupt trace at a `--trace` window, or the WS281x
#            frames decoded off the pad at a longer one
#   stdout   the run report
#   stderr*  everything on stderr except two masked lines, below
#
# Two stderr lines are masked, and each for a stated reason:
#
#   `blocks: …`  the INTERPRETER's block-cache counters. They MUST differ —
#                translated code serves the entries the cache would have
#                served — so the line is dropped and the raw diff printed
#                beside the table instead of being silently tolerated.
#   `jit: …`     exists only on the `--jit --jit-report` leg and says what was
#                translated. That is not guest state at all. Printed in full.
#
# # Why this is not silent when it cannot run
#
# A binary built without `--features jit` still accepts `--jit` and then does
# nothing useful with it, which would make this script compare a run against
# itself and pass. So the jit leg's report is checked for the coverage line
# the translator prints, and its absence is a failure with a named reason —
# never a skip.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

slug="${1:-harness}"
grade="${2:-t2}"
window="${3:-20ms}"
out="${4:-target/jit-identity}"

bin="${LP_EMU_JIT_IDENTITY_BIN:-target/release/lp-emu-esp32c6}"

# The same rows `oracle-sweep.sh` and `bench-web.sh` carry, in the same shape
# and with the same pins. A copy rather than a source, for the reason
# `oracle-sweep.sh` gives: those files run things when sourced.
#
# slug|env var|features|commit|spike
images=(
    "harness|LP_EMU_C6_REF_HARNESS|test_shader_compile_incremental,esp32c6,spike_uart0_link|d6cfaa205|e8d64eeff"
    "boot-idle-memfs|LP_EMU_C6_REF_BOOT_IDLE_MEMFS|esp32c6,server,radio,spike_uart0_link,memory_fs|d6cfaa205|e8d64eeff"
    "render-basic|LP_EMU_C6_REF_RENDER_BASIC|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_render_loop|77384a894|none"
    "render-rocaille|LP_EMU_C6_REF_RENDER_ROCAILLE|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_project_rocaille|77384a894|none"
)

elf=""
for spec in "${images[@]}"; do
    IFS='|' read -r s var features commit spike <<<"$spec"
    [[ "$s" == "$slug" ]] || continue
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "jit-identity-image: $var points at $path, which is not a file" >&2; exit 1; }
        elf="$path"
    else
        path="target/emu-ref/$commit-$slug/fw-esp32c6"
        if [[ ! -f "$path" ]]; then
            echo "jit-identity-image: building the $slug reference image" >&2
            scripts/emu/build-reference-image.sh "$features" "$commit" "$spike" >&2
        fi
        elf="$path"
    fi
done
if [[ -z "$elf" ]]; then
    echo "jit-identity-image: no pinned image called \"$slug\". Known slugs:" >&2
    for spec in "${images[@]}"; do IFS='|' read -r s _ <<<"$spec"; echo "    $s" >&2; done
    exit 2
fi

[[ -x "$bin" ]] || {
    echo "jit-identity-image: $bin is not an executable." >&2
    echo "  Build it with: cargo build --release -p lp-emu-esp32c6 --features jit" >&2
    exit 1
}

mkdir -p "$out"
stem="$out/$slug-$grade-$window"

leg() {
    local name="$1"; shift
    local extra=()
    # A `--trace` is the third reading at a short window and a frame dump at a
    # long one: the trace is every MMIO access and every interrupt, which is
    # the strongest of the three and also the most expensive per emulated
    # millisecond. 50 ms is the line, and the caller picks which side of it to
    # be on by picking the window.
    if [[ "$window" =~ ^([0-9]+)ms$ ]] && (( BASH_REMATCH[1] <= 50 )); then
        extra=(--trace --trace-file "$stem-$name.trace")
    fi
    # The leg's own stderr is where the emulator says why it would not start —
    # `--jit` is not even a recognised flag without `--features jit`, and under
    # `set -e` that would otherwise be a bare non-zero exit with nothing said.
    if ! "$bin" --elf "$elf" --time-grade "$grade" --wall-timeout 1800 --timeout "$window" \
        --uart0 "file:$stem-$name.uart" --dump-frames "file:$stem-$name.jsonl" \
        ${extra[@]+"${extra[@]}"} "$@" >"$stem-$name.out" 2>"$stem-$name.err"; then
        echo "jit-identity-image: the $name leg exited non-zero. Its last lines:" >&2
        tail -10 "$stem-$name.err" >&2
        if [[ "$name" == jit ]]; then
            echo "  If that names --jit as unknown, this binary was built WITHOUT" >&2
            echo "  --features jit: cargo build --release -p lp-emu-esp32c6 --features jit" >&2
        fi
        exit 1
    fi
}

echo "jit-identity-image: $slug $grade $window through $bin"
echo "jit-identity-image:   elf $elf ($(shasum -a 256 "$elf" 2>/dev/null | cut -c1-16 || sha256sum "$elf" | cut -c1-16))"
leg jit --jit --jit-report
leg interp --interpreter

# The proof that the jit leg actually translated something. Without
# `--features jit` the flag is accepted and nothing is emitted, and then this
# script would be comparing a run against itself — green, and worth nothing.
if ! grep -aq '^jit: coverage ' "$stem-jit.err"; then
    echo "jit-identity-image: the --jit leg printed no coverage line, so nothing was translated." >&2
    echo "  This binary is almost certainly built WITHOUT --features jit. That is a FAILURE," >&2
    echo "  not a skip: a self-comparison would pass and prove nothing." >&2
    tail -5 "$stem-jit.err" >&2
    exit 1
fi

mask() { grep -av -e '^blocks: ' -e '^jit: ' "$1"; }
mask "$stem-jit.err" >"$stem-jit.err.masked"
mask "$stem-interp.err" >"$stem-interp.err.masked"

fail=0
# `same` is called in a command substitution, which is a SUBSHELL — a `fail=1`
# set inside one is lost. So the verdict comes back as the word and the caller
# sets the flag. (Written the other way first; the script then reported
# `DIFFER` in the table and exited 0.)
same() { if cmp -s "$1" "$2"; then echo same; else echo DIFFER; fi; }
note() { [[ "$1" == same ]] || fail=1; }
stopped_of() { grep -ha 'stopped after ' "$1" "$2" | head -1; }

if [[ -f "$stem-jit.trace" ]]; then third_a="$stem-jit.trace"; third_b="$stem-interp.trace"; third_name=trace
else third_a="$stem-jit.jsonl"; third_b="$stem-interp.jsonl"; third_name=frames; fi

v_uart="$(same "$stem-jit.uart" "$stem-interp.uart")"; note "$v_uart"
if [[ "$(stopped_of "$stem-jit.out" "$stem-jit.err")" == "$(stopped_of "$stem-interp.out" "$stem-interp.err")" ]]; then
    v_stopped=same
else
    v_stopped=DIFFER; fail=1
fi
v_third="$(same "$third_a" "$third_b")"; note "$v_third"
v_out="$(same "$stem-jit.out" "$stem-interp.out")"; note "$v_out"
v_err="$(same "$stem-jit.err.masked" "$stem-interp.err.masked")"; note "$v_err"

echo
printf '%-16s %-5s %-7s %8s %9s %8s %8s %9s\n' image grade window uart stopped "$third_name" stdout "stderr*"
printf '%-16s %-5s %-7s %8s %9s %8s %8s %9s\n' "$slug" "$grade" "$window" \
    "$v_uart" "$v_stopped" "$v_third" "$v_out" "$v_err"
echo
echo "n: $(wc -l <"$third_a" | tr -d ' ') $third_name line(s)"
echo "stopped: $(stopped_of "$stem-jit.out" "$stem-jit.err")"
grep -a '^jit: coverage' "$stem-jit.err" || true
echo "--- the masked block-cache line, raw (it MUST differ) ---"
diff <(grep -a '^blocks: ' "$stem-jit.err") <(grep -a '^blocks: ' "$stem-interp.err") || true

if (( fail )); then
    echo
    echo "jit-identity-image: NOT IDENTICAL — translated and interpreted disagree. This is a stop." >&2
    exit 1
fi
echo
echo "jit-identity-image: identical on all five readings."
