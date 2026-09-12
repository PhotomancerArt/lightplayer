#!/usr/bin/env bash
# The classic ESP32 (v3) machine's speed probe — an ORACLE, never a gate.
#
#   just bench-emu-esp32v3                             # build, run, table, promote to prev/
#   scripts/emu/bench-esp32v3.sh --json out.json       # same, plus machine-readable
#   scripts/emu/bench-esp32v3.sh --bin <path> --no-promote --no-build
#
# `scripts/emu/bench-c6.sh`'s twin, and everything in that script's header
# applies here — read it too. What is different about this chip is written
# down below.
#
# Runs three pinned reference images (`scripts/emu/build-reference-image.sh
# --chip esp32`, building them if they are missing; override with
# `LP_EMU_ESP32V3_REF_BOOT_IDLE`, `LP_EMU_ESP32V3_REF_SHADER_COMPILE_STRESS`,
# `LP_EMU_ESP32V3_REF_RENDER_LOOP`) at **t1**, twice each, and reports the
# best run of each pair.
#
# ⚠️ THE THREE IMAGES ARE NOT INTERCHANGEABLE, and quoting one of them as
# "the classic emulator's speed" is how the C6's ladder spent four milestones
# measuring the wrong thing. The three, and what each is:
#
#   boot-idle              the shipped image, no project. `waiti` with the
#                          heartbeat's pacer underneath. IDLE — the fastest
#                          row and the least interesting one.
#   shader-compile-stress  the `test_shader_compile_incremental` harness:
#                          shader COMPILE, console-bound.
#   render-loop            THE PRODUCT'S RENDER LOOP — 241 lamps on IO18,
#                          the same project and the same pixels the C6's
#                          `render-basic` renders, retargeted D10 -> IO18 at
#                          build time. **This is the row to quote.**
#
# ⚠️ **t1 is the only grade this machine has** (plan decision E1, and the
# binary's own `--time-grade` help says so): a cycle IS an instruction here.
# The C6's script sweeps t1 and t2; this one does not, and a t2 column would
# be a column of the same numbers.
#
# ⚠️ **Both cores, at the default `--core-quantum 256`.** The classic is dual
# core and the run loop hands each unheld core a window per iteration (D3),
# so `instr/s` is reported per hart as well as in total — a single figure
# would hide that core 1 spends almost all of its time parked in `waiti`.
# The quantum is a run parameter: two quanta are two interleavings whose
# cycle counts may legitimately differ, so a number taken at another quantum
# is not comparable with these.
#
#   user s        USER CPU seconds — the only number that survives a busy
#                 machine. Compare THESE across binaries.
#   wall s        host wall clock, inflated by anything else running.
#   instr/s       instructions / user s, in total and per hart.
#   rt(user)      emulated µs / user s — how close the run is to the 240 MHz
#                 part it models, on the CPU time it actually got. This is
#                 the comparable ratio, and 240 MHz is where it comes from:
#                 the machine's emulated µs are its cycles / 240
#                 (`memmap::CYCLES_PER_US`), so "against 240 MHz" is already
#                 in the number.
#   rt(wall)      the same over wall clock: what a person watching the run
#                 experiences, and therefore the real answer on an IDLE
#                 machine and meaningless on a busy one. The load average
#                 sits next to it so nobody quotes it without that context.
#   uart          `cmp` of this run's UART0 bytes against `prev/`, which is
#                 the identity oracle — speed work must not change one byte
#                 of any transcript.
#
# A meaningful before/after is a SAME-WINDOW A/B: run the saved stock binary
# with `--bin ... --no-promote --no-build`, then the new one, back to back,
# and quote the load average with both. Wall-clock numbers taken hours apart,
# or under another agent's build, are not speed results.
#
# **Where the SHARES come from, and why not from here.** This script measures
# seconds. It does not measure where they go: that is
# `scripts/emu/selfprof-buckets.py` (host time, from the `selfprof` pc
# sampler) and `scripts/emu/bench-esp32v3-counts.py` (MMIO sites, window
# exceptions, LOOP hotness, the interleave's switch rate — from the `bench`
# build's counters). Both of those builds are SLOWER than this one by
# construction, so their seconds mean nothing and only their shares do. Keep
# the two apart.
#
# Emulated microseconds never gate anything (AGENTS.md): transcripts decide,
# probes report.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

bench_dir="target/emu-bench-esp32v3"
prev_dir="$bench_dir/prev"
bin="target/release/lp-emu-esp32v3"
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
        -h|--help) sed -n '2,78p' "$0"; exit 0 ;;
        *) echo "bench-esp32v3: unknown option $1" >&2; exit 2 ;;
    esac
done

mkdir -p "$bench_dir"

# --- the reference images ---------------------------------------------------
# slug|env var|features|emulated timeout|--exit-on substring (empty = none)|commit
#
# Every row names its own commit, because there is no historical pin a clean
# tree can reproduce for this chip (build-reference-image.sh's ruling R7) and
# because `bench_render_loop` does not exist before the commit below. The
# classic takes no `spike_uart0_link` cherry-pick — its host link IS UART0 —
# so unlike the C6's table there is no spike column.
#
# `boot-idle` runs to a fixed emulated deadline rather than to its sentinel:
# an idle image's sentinel arrives in the first few hundred milliseconds and
# the rest of the row would be measuring the idle skip. A fixed span is the
# same span every run.
images=(
    "boot-idle|LP_EMU_ESP32V3_REF_BOOT_IDLE|esp32,server,float-f32|3s||0773c3fbd"
    "shader-compile-stress|LP_EMU_ESP32V3_REF_SHADER_COMPILE_STRESS|esp32,test_shader_compile_incremental|60s|[inc-shader-compile] === DONE ===|0773c3fbd"
    "render-loop|LP_EMU_ESP32V3_REF_RENDER_LOOP|esp32,server,float-f32,bench_render_loop|20s|[render-loop] === DONE ===|0773c3fbd"
)

resolve_image() {
    local slug="$1" var="$2" features="$3" commit="$4" path
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "bench-esp32v3: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        return
    fi
    path="target/emu-ref/$commit-$slug/fw-esp32v3"
    if [[ ! -f "$path" ]]; then
        echo "bench-esp32v3: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh --chip esp32 "$features" "$commit" none >&2
    fi
    echo "$path"
}

if [[ $do_build -eq 1 ]]; then
    echo "bench-esp32v3: cargo build -p lp-emu-esp32v3 --release" >&2
    cargo build -p lp-emu-esp32v3 --release >&2
fi
[[ -x "$bin" ]] || { echo "bench-esp32v3: $bin is not an executable" >&2; exit 1; }

loadavg() { sysctl -n vm.loadavg | awk '{print $2}'; }

grade=t1
rows=()
lines=()
json_rows=()

for spec in "${images[@]}"; do
    IFS='|' read -r slug var features timeout exit_on commit <<<"$spec"
    elf="$(resolve_image "$slug" "$var" "$features" "$commit")"

    uart="$bench_dir/$slug-$grade.txt"
    best_user=""
    best_real=""
    best_load=""
    summary=""
    emulated=""

    for _ in $(seq "$runs"); do
        out="$(mktemp)"
        load="$(loadavg)"
        set +e
        if [[ -n "$exit_on" ]]; then
            /usr/bin/time -p "$bin" --elf "$elf" --timeout "$timeout" \
                --wall-timeout 900 --exit-on "$exit_on" \
                --uart0 "file:$uart" --time-grade "$grade" >"$out" 2>&1
        else
            /usr/bin/time -p "$bin" --elf "$elf" --timeout "$timeout" \
                --wall-timeout 900 \
                --uart0 "file:$uart" --time-grade "$grade" >"$out" 2>&1
        fi
        rc=$?
        set -e
        if [[ $rc -ne 0 ]]; then
            echo "bench-esp32v3: $slug exited $rc" >&2
            tail -40 "$out" >&2
            rm -f "$out"
            exit 1
        fi
        # `run: cycles=… instructions=… (core0=… core1=…) idle=… …` — this
        # machine's summary line, the classic's answer to the C6's
        # `stopped after`. The emulated microseconds are on the outcome line
        # above it, whichever outcome it was.
        summary="$(grep -m1 '^run: cycles=' "$out" || true)"
        [[ -n "$summary" ]] || { echo "bench-esp32v3: no 'run: cycles=' line for $slug" >&2; tail -40 "$out" >&2; exit 1; }
        emulated="$(sed -nE 's/.*\(([0-9]+) us emulated\).*/\1/p' "$out" | head -1)"
        [[ -n "$emulated" ]] || { echo "bench-esp32v3: no 'us emulated' figure for $slug" >&2; tail -40 "$out" >&2; exit 1; }
        user="$(awk '/^user /{print $2}' "$out")"
        real="$(awk '/^real /{print $2}' "$out")"
        rm -f "$out"
        if [[ -z "$best_user" ]] || awk "BEGIN{exit !($user < $best_user)}"; then
            best_user="$user"; best_real="$real"; best_load="$load"
        fi
    done

    cycles="$(sed -nE 's/^run: cycles=([0-9]+).*/\1/p' <<<"$summary")"
    instr="$(sed -nE 's/.*instructions=([0-9]+).*/\1/p' <<<"$summary")"
    core0="$(sed -nE 's/.*core0=([0-9]+).*/\1/p' <<<"$summary")"
    core1="$(sed -nE 's/.*core1=([0-9]+).*/\1/p' <<<"$summary")"

    instr_s=$(awk -v i="$instr" -v u="$best_user" 'BEGIN{printf "%.1f", i / u / 1e6}')
    instr_raw=$(awk -v i="$instr" -v u="$best_user" 'BEGIN{printf "%.0f", i / u}')
    c0_s=$(awk -v i="$core0" -v u="$best_user" 'BEGIN{printf "%.1f", i / u / 1e6}')
    c1_s=$(awk -v i="$core1" -v u="$best_user" 'BEGIN{printf "%.1f", i / u / 1e6}')
    rt_wall=$(awk -v e="$emulated" -v w="$best_real" 'BEGIN{printf "%.3f", e / 1e6 / w}')
    rt_user=$(awk -v e="$emulated" -v u="$best_user" 'BEGIN{printf "%.3f", e / 1e6 / u}')

    if [[ -f "$prev_dir/$slug-$grade.txt" ]]; then
        if cmp -s "$prev_dir/$slug-$grade.txt" "$uart"; then uart_cmp="same"; else uart_cmp="DIFFER"; fi
    else
        uart_cmp="no prev"
    fi

    rows+=("$(printf '%-22s %-5s %8s %8s %9s %9s %9s %9s %9s %7s %8s' \
        "$slug" "$grade" "$best_user" "$best_real" "${instr_s}M" \
        "${c0_s}M" "${c1_s}M" "${rt_user}x" "${rt_wall}x" "$best_load" "$uart_cmp")")
    lines+=("$slug $grade: $summary ($emulated us emulated)")
    json_rows+=("$(printf '{"image":"%s","grade":"%s","user_s":%s,"wall_s":%s,"instr_per_s":%s,"cycles":%s,"emulated_us":%s,"instructions":%s,"instructions_core0":%s,"instructions_core1":%s,"real_time_user":%s,"real_time_wall":%s,"loadavg_1m":%s,"uart_cmp":"%s"}' \
        "$slug" "$grade" "$best_user" "$best_real" "$instr_raw" \
        "$cycles" "$emulated" "$instr" "$core0" "$core1" "$rt_user" "$rt_wall" "$best_load" "$uart_cmp")")
done

echo
printf '%-22s %-5s %8s %8s %9s %9s %9s %9s %9s %7s %8s\n' \
    image grade "user s" "wall s" "instr/s" "core0/s" "core1/s" "rt(user)" "rt(wall)" load uart
printf '%-22s %-5s %8s %8s %9s %9s %9s %9s %9s %7s %8s\n' \
    ---------------------- ----- -------- -------- --------- --------- --------- --------- --------- ------- --------
for row in "${rows[@]}"; do echo "$row"; done
echo
for line in "${lines[@]}"; do echo "$line"; done
echo
echo "binary: $bin ($(wc -c <"$bin" | tr -d ' ') bytes), best of $runs runs per row, both cores at the default quantum"
echo "rt(*) is against 240 MHz — the machine's emulated us ARE its cycles / 240."
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
        IFS='|' read -r slug _ _ _ _ _ <<<"$spec"
        cp "$bench_dir/$slug-$grade.txt" "$prev_dir/$slug-$grade.txt"
    done
    echo "promoted this run's UART output to $prev_dir/ (the next run compares against it)"
fi
