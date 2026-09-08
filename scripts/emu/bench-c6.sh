#!/usr/bin/env bash
# The ESP32-C6 machine's speed probe — an ORACLE, never a gate.
#
#   just bench-emu-c6                          # build, run, table, promote to prev/
#   scripts/emu/bench-c6.sh --json out.json    # same, plus machine-readable
#   scripts/emu/bench-c6.sh --bin <path> --no-promote --no-build
#
# Runs the four pinned reference images (`scripts/emu/build-reference-image.sh`,
# building them if they are missing, same env-var convention as
# `just test-emu-c6`: `LP_EMU_C6_REF_HARNESS`, `LP_EMU_C6_REF_BOOT_IDLE_MEMFS`,
# `LP_EMU_C6_REF_RENDER_BASIC`, `LP_EMU_C6_REF_RENDER_ROCAILLE`) at both time
# grades, twice each, and reports the best run of each pair:
#
# ⚠️ THE FOUR IMAGES ARE NOT INTERCHANGEABLE, and quoting one of them as "the
# emulator's speed" is how this ladder spent four milestones measuring the
# wrong thing. Measured 2026-09-08 on one loaded Mac, same window:
#
#   boot-idle-memfs   6.0x real time   — `wfi` with no project. Idle.
#   harness           0.6x / 3.2x      — shader COMPILE, and console-bound:
#                                        the M4 poll skip moves it 5.6x.
#   render-basic      0.47x            — the product's render loop. The skip
#                                        is worth nothing here, and slightly
#                                        NEGATIVE at t1.
#   render-rocaille   0.53x            — the same loop, 4x the shader.
#
# The render-loop rows are the ones the product cares about (M5 P0). They are
# ~12x slower than boot-idle and they are the number to quote.
#
#   user s        USER CPU seconds — the only number that survives a busy
#                 machine. Compare THESE across binaries.
#   wall s        host wall clock, inflated by anything else running.
#   instr/s       instructions / user s — the throughput figure.
#   rt(user)      emulated µs / user s — how close the run is to the silicon
#                 it models, on the CPU time it actually got. This is the
#                 comparable ratio.
#   rt(wall)      the same over wall clock: what a person watching the run
#                 experiences, and therefore the real answer on an IDLE
#                 machine and meaningless on a busy one. The load average
#                 sits next to it so nobody quotes it without that context.
#   uart          `cmp` of this run's UART0 bytes against `prev/`, which is
#                 the identity oracle — the speed work must not change one
#                 byte of any transcript (PD5, ADR 2026-09-06).
#
# A meaningful before/after is a SAME-WINDOW A/B: run the saved stock binary
# with `--bin ... --no-promote --no-build`, then the new one, back to back,
# and quote the load average with both. Wall-clock numbers taken hours apart,
# or under another agent's build, are not speed results.
#
# Emulated microseconds never gate anything (AGENTS.md "The ESP32-C6
# emulator"): transcripts decide, probes report.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

bench_dir="target/emu-bench"
prev_dir="$bench_dir/prev"
bin="target/release/lp-emu-esp32c6"
json_out=""
do_build=1
do_promote=1
runs=2

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json) json_out="${2:?--json needs a path}"; shift 2 ;;
        --bin) bin="${2:?--bin needs a path}"; do_build=0; shift 2 ;;
        --no-build) do_build=0; shift ;;
        --no-promote) do_promote=0; shift ;;
        --runs) runs="${2:?--runs needs a count}"; shift 2 ;;
        -h|--help) sed -n '2,32p' "$0"; exit 0 ;;
        *) echo "bench-c6: unknown option $1" >&2; exit 2 ;;
    esac
done

mkdir -p "$bench_dir"

# --- the reference images ---------------------------------------------------
# slug|env var|features|emulated timeout|--exit-on substring (empty = none)
# slug|env var|features|emulated timeout|--exit-on substring|commit|spike
#
# The last two columns are per image. The three original rows carry the
# historical pin the committed transcripts were recorded at; the two
# render-loop rows CANNOT — `bench_render_loop` does not exist at d6cfaa205 —
# so they name their own commit and take no cherry-pick (`none`), because the
# spike feature is already in their tree.
images=(
    "harness|LP_EMU_C6_REF_HARNESS|test_shader_compile_incremental,esp32c6,spike_uart0_link|5s|[inc-shader-compile] === DONE ===|d6cfaa205|e8d64eeff"
    "boot-idle-memfs|LP_EMU_C6_REF_BOOT_IDLE_MEMFS|esp32c6,server,radio,spike_uart0_link,memory_fs|3s||d6cfaa205|e8d64eeff"
    "render-basic|LP_EMU_C6_REF_RENDER_BASIC|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_render_loop|8s|[render-loop] === DONE ===|8ffc4b325|none"
    "render-rocaille|LP_EMU_C6_REF_RENDER_ROCAILLE|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_project_rocaille|8s|[render-loop] === DONE ===|8ffc4b325|none"
)

resolve_image() {
    local slug="$1" var="$2" features="$3" commit="$4" spike="$5" path
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "bench-c6: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        return
    fi
    path="target/emu-ref/$commit-$slug/fw-esp32c6"
    if [[ ! -f "$path" ]]; then
        echo "bench-c6: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh "$features" "$commit" "$spike" >&2
    fi
    echo "$path"
}

if [[ $do_build -eq 1 ]]; then
    echo "bench-c6: cargo build -p lp-emu-esp32c6 --release" >&2
    cargo build -p lp-emu-esp32c6 --release >&2
fi
[[ -x "$bin" ]] || { echo "bench-c6: $bin is not an executable" >&2; exit 1; }

loadavg() { sysctl -n vm.loadavg | awk '{print $2}'; }

rows=()
lines=()
json_rows=()

for spec in "${images[@]}"; do
    IFS='|' read -r slug var features timeout exit_on commit spike <<<"$spec"
    elf="$(resolve_image "$slug" "$var" "$features" "$commit" "$spike")"

    for grade in t1 t2; do
        uart="$bench_dir/$slug-$grade.txt"
        best_user=""
        best_real=""
        best_load=""
        stopped=""

        for _ in $(seq "$runs"); do
            out="$(mktemp)"
            load="$(loadavg)"
            set +e
            if [[ -n "$exit_on" ]]; then
                /usr/bin/time -p "$bin" --elf "$elf" --timeout "$timeout" \
                    --wall-timeout 600 --exit-on "$exit_on" \
                    --uart0 "file:$uart" --time-grade "$grade" >"$out" 2>&1
            else
                /usr/bin/time -p "$bin" --elf "$elf" --timeout "$timeout" \
                    --wall-timeout 600 \
                    --uart0 "file:$uart" --time-grade "$grade" >"$out" 2>&1
            fi
            rc=$?
            set -e
            if [[ $rc -ne 0 ]]; then
                echo "bench-c6: $slug $grade exited $rc" >&2
                cat "$out" >&2
                rm -f "$out"
                exit 1
            fi
            stopped="$(grep -m1 '^stopped after ' "$out" || true)"
            [[ -n "$stopped" ]] || { echo "bench-c6: no 'stopped after' line for $slug $grade" >&2; cat "$out" >&2; exit 1; }
            user="$(awk '/^user /{print $2}' "$out")"
            real="$(awk '/^real /{print $2}' "$out")"
            rm -f "$out"
            if [[ -z "$best_user" ]] || awk "BEGIN{exit !($user < $best_user)}"; then
                best_user="$user"; best_real="$real"; best_load="$load"
            fi
        done

        # stopped after N cycles (U us emulated, I instructions, grade G)
        read -r cycles us instr <<<"$(sed -E 's/^stopped after ([0-9]+) cycles \(([0-9]+) us emulated, ([0-9]+) instructions.*/\1 \2 \3/' <<<"$stopped")"

        instr_s=$(awk -v i="$instr" -v u="$best_user" 'BEGIN{printf "%.1f", i / u / 1e6}')
        instr_raw=$(awk -v i="$instr" -v u="$best_user" 'BEGIN{printf "%.0f", i / u}')
        rt_wall=$(awk -v e="$us" -v w="$best_real" 'BEGIN{printf "%.2f", e / 1e6 / w}')
        rt_user=$(awk -v e="$us" -v u="$best_user" 'BEGIN{printf "%.2f", e / 1e6 / u}')

        if [[ -f "$prev_dir/$slug-$grade.txt" ]]; then
            if cmp -s "$prev_dir/$slug-$grade.txt" "$uart"; then uart_cmp="same"; else uart_cmp="DIFFER"; fi
        else
            uart_cmp="no prev"
        fi

        rows+=("$(printf '%-16s %-5s %8s %8s %10s %9s %9s %7s %8s' \
            "$slug" "$grade" "$best_user" "$best_real" "${instr_s}M" \
            "${rt_user}x" "${rt_wall}x" "$best_load" "$uart_cmp")")
        lines+=("$slug $grade: $stopped")
        json_rows+=("$(printf '{"image":"%s","grade":"%s","user_s":%s,"wall_s":%s,"instr_per_s":%s,"cycles":%s,"emulated_us":%s,"instructions":%s,"real_time_user":%s,"real_time_wall":%s,"loadavg_1m":%s,"uart_cmp":"%s"}' \
            "$slug" "$grade" "$best_user" "$best_real" "$instr_raw" \
            "$cycles" "$us" "$instr" "$rt_user" "$rt_wall" "$best_load" "$uart_cmp")")
    done
done

echo
printf '%-16s %-5s %8s %8s %10s %9s %9s %7s %8s\n' image grade "user s" "wall s" "instr/s" "rt(user)" "rt(wall)" "load" uart
printf '%-16s %-5s %8s %8s %10s %9s %9s %7s %8s\n' ---------------- ----- -------- -------- ---------- --------- --------- ------- --------
for row in "${rows[@]}"; do echo "$row"; done
echo
for line in "${lines[@]}"; do echo "$line"; done
echo
echo "binary: $bin ($(wc -c <"$bin" | tr -d ' ') bytes), best of $runs runs per row"
echo "These numbers are ORACLES, not gates — never gate on emulated microseconds (AGENTS.md)."

if [[ -n "$json_out" ]]; then
    mkdir -p "$(dirname "$json_out")"
    {
        printf '{"binary":"%s","runs_per_row":%s,"rows":[' "$bin" "$runs"
        for i in "${!json_rows[@]}"; do
            [[ $i -gt 0 ]] && printf ','
            printf '%s' "${json_rows[$i]}"
        done
        printf ']}\n'
    } >"$json_out"
    echo "json: $json_out"
fi

if [[ $do_promote -eq 1 ]]; then
    mkdir -p "$prev_dir"
    for spec in "${images[@]}"; do
        IFS='|' read -r slug _ _ _ _ _ _ <<<"$spec"
        for grade in t1 t2; do
            cp "$bench_dir/$slug-$grade.txt" "$prev_dir/$slug-$grade.txt"
        done
    done
    echo "promoted this run's UART output to $prev_dir/ (the next run compares against it)"
fi
