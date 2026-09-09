#!/usr/bin/env bash
# The identity oracle: two emulator binaries must produce the same bytes.
#
#   scripts/emu/oracle-sweep.sh <binary-a> <binary-b> [out-dir]
#   scripts/emu/oracle-sweep.sh target/release/lp-emu-esp32c6 /path/to/other
#
# Runs every pinned reference image at both time grades through both binaries
# and compares three things per cell:
#
#   uart     the guest's UART0 bytes — what the firmware said
#   stopped  the `stopped after N cycles (U us, I instructions)` line — what
#            the machine counted
#   frames   the WS281x frames decoded off the PAD from `--dump-frames` — what
#            the wire carried
#
# The speed work must not change one byte of any of them (PD5, ADR
# 2026-09-06). A `DIFFER` in any column is a stop, not a note.
#
# # Why the frame column exists, and why it is not in `bench-c6.sh`
#
# `uart` and `stopped` are both the guest marking its own homework: they say
# what the firmware believed and what our own cycle accounting counted. The
# frame dump is the third, independent reading — our WS281x decoder reading
# the waveform the RMT model actually put on gpio18 — and it is the only one
# of the three that covers the OUTPUT path, which is exactly where a block
# cache's store-side invalidation is most likely to go quietly wrong.
#
# It is deliberately NOT in `bench-c6.sh`: decoding costs host time, and
# `bench-c6.sh` exists to measure host time. Identity here, speed there.
#
# # A frame dump is comparable within a grade, never across one
#
# `t1` and `t2` produce different frames from the same firmware, and that is
# correct rather than a bug. The render-loop images tick their projects on a
# FIXED delta, so the project's own clock is grade-independent — but the
# display pipeline's temporal interpolation reads the guest's real microsecond
# clock, and the two grades disagree about how many microseconds a frame took.
# So the sweep compares A against B within each (image, grade) cell and never
# across cells.
set -euo pipefail

bin_a="${1:?usage: oracle-sweep.sh <binary-a> <binary-b> [out-dir]}"
bin_b="${2:?usage: oracle-sweep.sh <binary-a> <binary-b> [out-dir]}"
out="${3:-target/emu-oracle}"

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

for bin in "$bin_a" "$bin_b"; do
    [[ -x "$bin" ]] || { echo "oracle-sweep: $bin is not an executable" >&2; exit 1; }
done
mkdir -p "$out"

# The same rows `bench-c6.sh` carries, in the same shape. Kept as a copy rather
# than sourced: `bench-c6.sh` runs things when sourced, and an oracle that
# started a benchmark as a side effect would be a bad oracle.
# slug|env var|features|emulated timeout|--exit-on substring|commit|spike
images=(
    "harness|LP_EMU_C6_REF_HARNESS|test_shader_compile_incremental,esp32c6,spike_uart0_link|5s|[inc-shader-compile] === DONE ===|d6cfaa205|e8d64eeff"
    "boot-idle-memfs|LP_EMU_C6_REF_BOOT_IDLE_MEMFS|esp32c6,server,radio,spike_uart0_link,memory_fs|5500ms||d6cfaa205|e8d64eeff"
    "render-basic|LP_EMU_C6_REF_RENDER_BASIC|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_render_loop|8s|[render-loop] === DONE ===|8ffc4b325|none"
    "render-rocaille|LP_EMU_C6_REF_RENDER_ROCAILLE|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_project_rocaille|8s|[render-loop] === DONE ===|8ffc4b325|none"
)

resolve_image() {
    local slug="$1" var="$2" features="$3" commit="$4" spike="$5" path
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "oracle-sweep: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        return
    fi
    path="target/emu-ref/$commit-$slug/fw-esp32c6"
    if [[ ! -f "$path" ]]; then
        echo "oracle-sweep: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh "$features" "$commit" "$spike" >&2
    fi
    echo "$path"
}

run_leg() {
    local bin="$1" elf="$2" slug="$3" grade="$4" timeout="$5" exit_on="$6" leg="$7"
    local stem="$out/$leg-$slug-$grade"
    local args=(--elf "$elf" --timeout "$timeout" --wall-timeout 900
                --uart0 "file:$stem.uart" --dump-frames "file:$stem.jsonl"
                --time-grade "$grade")
    [[ -n "$exit_on" ]] && args+=(--exit-on "$exit_on")
    # stderr carries the `stopped after` line; stdout is the run report.
    "$bin" "${args[@]}" >"$stem.out" 2>"$stem.err" || {
        echo "oracle-sweep: $leg $slug $grade exited non-zero" >&2
        tail -20 "$stem.err" >&2
        exit 1
    }
    grep -a '^stopped after ' "$stem.err" > "$stem.line" || {
        echo "oracle-sweep: no 'stopped after' line for $leg $slug $grade" >&2
        exit 1
    }
}

fail=0
rows=()
for spec in "${images[@]}"; do
    IFS='|' read -r slug var features timeout exit_on commit spike <<<"$spec"
    elf="$(resolve_image "$slug" "$var" "$features" "$commit" "$spike")"
    for grade in t1 t2; do
        run_leg "$bin_a" "$elf" "$slug" "$grade" "$timeout" "$exit_on" a
        run_leg "$bin_b" "$elf" "$slug" "$grade" "$timeout" "$exit_on" b

        verdict=()
        for what in uart line jsonl; do
            if cmp -s "$out/a-$slug-$grade.$what" "$out/b-$slug-$grade.$what"; then
                verdict+=("same")
            else
                verdict+=("DIFFER")
                fail=1
            fi
        done
        frames="$(wc -l < "$out/a-$slug-$grade.jsonl" | tr -d ' ')"
        rows+=("$(printf '%-16s %-5s %8s %9s %8s %8s' \
            "$slug" "$grade" "${verdict[0]}" "${verdict[1]}" "${verdict[2]}" "$frames")")
    done
done

echo
printf '%-16s %-5s %8s %9s %8s %8s\n' image grade uart stopped frames "n"
printf '%-16s %-5s %8s %9s %8s %8s\n' ---------------- ----- -------- --------- -------- --------
for row in "${rows[@]}"; do echo "$row"; done
echo
echo "a: $bin_a"
echo "b: $bin_b"
echo "artefacts: $out/"

if (( fail )); then
    echo
    echo "oracle-sweep: NOT IDENTICAL — the two binaries disagree. This is a stop." >&2
    exit 1
fi
echo
echo "oracle-sweep: identical on every image, both grades, all three readings."
