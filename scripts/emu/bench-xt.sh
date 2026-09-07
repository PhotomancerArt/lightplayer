#!/usr/bin/env bash
# The Xtensa core's speed probe — an ORACLE, never a gate.
#
#   just bench-emu-xt                          # build, run, table, promote to prev/
#   scripts/emu/bench-xt.sh --json out.json    # same, plus machine-readable
#   scripts/emu/bench-xt.sh --bin <path> --no-promote --no-build
#
# Sibling of `scripts/emu/bench-c6.sh`; same shape, same A/B protocol, same
# rule that the numbers report and never gate. The differences are forced by
# what the Xtensa core is:
#
#   * `lp-xt-emu` is an ISA core with no SoC around it — no peripherals, no
#     scheduler, no emulated clock. It runs at `CycleModel::InstructionCount`,
#     where a cycle IS an instruction, so there is no real-time ratio to
#     report: the columns are instructions per second, user seconds and wall
#     seconds, with the load average beside them.
#
#   * There is no long-running Xtensa image in this repo. The workload is the
#     `lp-xt/fixtures` corpus (built by `lp-xt/fixtures/build.sh` with the esp
#     toolchain), whose longest program retires 292 k instructions — three
#     orders of magnitude short of a probe-sized run. So the probe REPEATS
#     each program from a clean emulator until it has retired >=100 M
#     instructions. Per-repeat setup (region allocation plus the ELF load) is
#     ~40 us against ~16 ms of execution, so it is a rounding error in the
#     rate, not a hidden constant. If a long-running image ever exists, it
#     replaces the repeats and this note goes with them.
#
# Columns:
#
#   user s        USER CPU seconds — the only number that survives a busy
#                 machine. Compare THESE across binaries.
#   wall s        host wall clock, inflated by anything else running.
#   instr/s       instructions / user s — the throughput figure.
#   load          1-minute load average when the best run started, so nobody
#                 quotes a wall-clock number without its context.
#   out           `cmp` of the guest's collected output against `prev/`.
#   trace         `cmp` of a capped text trace (`--trace-lines`) of the first
#                 repeat against `prev/`. Together with `out` this is the
#                 identity oracle: the speed work must not change one byte of
#                 either.
#
# A meaningful before/after is a SAME-WINDOW A/B: run the saved stock binary
# with `--bin ... --no-promote --no-build`, then the new one, back to back,
# and quote the load average with both. Wall-clock numbers taken hours apart,
# or under another agent's build, are not speed results.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

bench_dir="target/emu-bench-xt"
prev_dir="$bench_dir/prev"
bin="target/release/xt-bench"
elf_dir="lp-xt/fixtures/elf"
json_out=""
do_build=1
do_promote=1
runs=2
trace_lines=200000

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json) json_out="${2:?--json needs a path}"; shift 2 ;;
        --bin) bin="${2:?--bin needs a path}"; do_build=0; shift 2 ;;
        --no-build) do_build=0; shift ;;
        --no-promote) do_promote=0; shift ;;
        --runs) runs="${2:?--runs needs a count}"; shift 2 ;;
        --trace-lines) trace_lines="${2:?--trace-lines needs a count}"; shift 2 ;;
        -h|--help) sed -n '2,45p' "$0"; exit 0 ;;
        *) echo "bench-xt: unknown option $1" >&2; exit 2 ;;
    esac
done

mkdir -p "$bench_dir"

# --- the workload ----------------------------------------------------------
# slug|repeats  — repeats chosen so each row retires >=100 M instructions.
# ackermann      292,394 instructions/run: deep recursion, heavy window
#                spill/reload traffic (the windowed-ABI stress case).
# fib_rec        137,527 instructions/run: a wide call tree, shallower frames.
# They are the only two fixtures long enough to be worth repeating; the rest
# of the corpus retires a few thousand instructions each and is covered by
# `cargo test -p lp-xt-elf` instead.
images=(
    "ackermann|400"
    "fib_rec|800"
)

if ! ls "$elf_dir"/*.elf >/dev/null 2>&1; then
    echo "bench-xt: building the fixture corpus (esp toolchain)" >&2
    lp-xt/fixtures/build.sh >&2
fi

if [[ $do_build -eq 1 ]]; then
    echo "bench-xt: cargo build -p lp-xt-elf --release --bin xt-bench" >&2
    cargo build -p lp-xt-elf --release --bin xt-bench >&2
fi
[[ -x "$bin" ]] || { echo "bench-xt: $bin is not an executable" >&2; exit 1; }

loadavg() { sysctl -n vm.loadavg | awk '{print $2}'; }

rows=()
lines=()
json_rows=()

for spec in "${images[@]}"; do
    IFS='|' read -r slug repeats <<<"$spec"
    elf="$elf_dir/$slug.elf"
    [[ -f "$elf" ]] || { echo "bench-xt: $elf missing — run lp-xt/fixtures/build.sh" >&2; exit 1; }

    out_file="$bench_dir/$slug.out"
    trace_file="$bench_dir/$slug.trace"
    best_user=""
    best_real=""
    best_load=""
    stopped=""

    for _ in $(seq "$runs"); do
        out="$(mktemp)"
        load="$(loadavg)"
        set +e
        /usr/bin/time -p "$bin" --elf "$elf" --repeat "$repeats" \
            --out "$out_file" --trace "$trace_file" --trace-lines "$trace_lines" \
            >"$out" 2>&1
        rc=$?
        set -e
        if [[ $rc -ne 0 ]]; then
            echo "bench-xt: $slug exited $rc" >&2
            cat "$out" >&2
            rm -f "$out"
            exit 1
        fi
        stopped="$(grep -m1 '^stopped after ' "$out" || true)"
        [[ -n "$stopped" ]] || { echo "bench-xt: no 'stopped after' line for $slug" >&2; cat "$out" >&2; exit 1; }
        user="$(awk '/^user /{print $2}' "$out")"
        real="$(awk '/^real /{print $2}' "$out")"
        rm -f "$out"
        if [[ -z "$best_user" ]] || awk "BEGIN{exit !($user < $best_user)}"; then
            best_user="$user"; best_real="$real"; best_load="$load"
        fi
    done

    # stopped after N cycles (I instructions, R repeats)
    read -r cycles instr <<<"$(sed -E 's/^stopped after ([0-9]+) cycles \(([0-9]+) instructions.*/\1 \2/' <<<"$stopped")"

    instr_s=$(awk -v i="$instr" -v u="$best_user" 'BEGIN{printf "%.1f", i / u / 1e6}')
    instr_raw=$(awk -v i="$instr" -v u="$best_user" 'BEGIN{printf "%.0f", i / u}')

    cmp_of() {
        local name="$1" live="$2"
        if [[ -f "$prev_dir/$name" ]]; then
            if cmp -s "$prev_dir/$name" "$live"; then echo "same"; else echo "DIFFER"; fi
        else
            echo "no prev"
        fi
    }
    out_cmp="$(cmp_of "$slug.out" "$out_file")"
    trace_cmp="$(cmp_of "$slug.trace" "$trace_file")"

    rows+=("$(printf '%-16s %8s %8s %8s %10s %7s %8s %8s' \
        "$slug" "$repeats" "$best_user" "$best_real" "${instr_s}M" \
        "$best_load" "$out_cmp" "$trace_cmp")")
    lines+=("$slug: $stopped")
    json_rows+=("$(printf '{"image":"%s","repeats":%s,"user_s":%s,"wall_s":%s,"instr_per_s":%s,"cycles":%s,"instructions":%s,"loadavg_1m":%s,"out_cmp":"%s","trace_cmp":"%s"}' \
        "$slug" "$repeats" "$best_user" "$best_real" "$instr_raw" \
        "$cycles" "$instr" "$best_load" "$out_cmp" "$trace_cmp")")
done

echo
printf '%-16s %8s %8s %8s %10s %7s %8s %8s\n' image repeats "user s" "wall s" "instr/s" load out trace
printf '%-16s %8s %8s %8s %10s %7s %8s %8s\n' ---------------- -------- -------- -------- ---------- ------- -------- --------
for row in "${rows[@]}"; do echo "$row"; done
echo
for line in "${lines[@]}"; do echo "$line"; done
echo
echo "binary: $bin ($(wc -c <"$bin" | tr -d ' ') bytes), best of $runs runs per row"
echo "CycleModel::InstructionCount — a cycle is an instruction; there is no emulated clock here."
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
        IFS='|' read -r slug _ <<<"$spec"
        cp "$bench_dir/$slug.out" "$prev_dir/$slug.out"
        cp "$bench_dir/$slug.trace" "$prev_dir/$slug.trace"
    done
    echo "promoted this run's output and trace to $prev_dir/ (the next run compares against it)"
fi
