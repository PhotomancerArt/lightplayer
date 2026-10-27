#!/usr/bin/env bash
# What one lane (a fixture and the output it feeds) costs per steady frame on
# the RV32 engine emulator: `lp-cli profile` steady render of two projects
# with the same LEDs and different lane counts, differenced.
#
#   per lane = (B - A) / (lanes_B - lanes_A)        fixed = A - lanes_A * per lane
#
# A = projects/test/lanes-2x384, B = projects/test/lanes-6x128 (768 LEDs each;
# the 8-lane projects do not fit the engine emulator's 320 KiB heap). Cycles
# are emulated (`--cycle-model esp32-c6`, configuration lp-emu:esp32c6-engine)
# and are a direction, never a device number: silicon underprices or
# overprices this walk by its own fetch cost. Allocations are the
# steady-render frame window's `alloc_count`, the figure the heap-budget gate
# ratchets for the same two projects.
#
# Usage: scripts/perf/per-lane-cost.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

# shellcheck disable=SC2206
LP_CLI_CMD=(${LP_CLI:-cargo run -q -p lp-cli --})
A=projects/test/lanes-2x384
B=projects/test/lanes-6x128
LANES_A=2
LANES_B=6

profile() {
    local project="$1" collect="$2"
    "${LP_CLI_CMD[@]}" profile "$project" --collect "$collect" --mode steady-render \
        --max-cycles 400000000 2>/dev/null | tail -1
}

# Mean cycles of the steady frames: every complete `frame` window after the
# first three (the compile frame and the warm-up). The cpu collector alone,
# because the alloc collector walks the guest's free list at every marker and
# bills it to the frame.
frame_cycles() {
    local dir
    dir="$(profile "$1" cpu)"
    jq -s '[ . as $e | range(0; length) as $i | $e[$i]
             | select(.name == "frame") ] as $f
           | [ range(0; $f | length; 1) as $i
               | select($f[$i].kind == "B" and ($f[$i + 1].kind // "") == "E")
               | $f[$i + 1].cycle - $f[$i].cycle ]
           | .[3:] | (add / length | floor)' "$dir/events.jsonl"
}

frame_allocs() {
    local dir
    dir="$(profile "$1" alloc)"
    jq '.windows[] | select(.name == "frame") | .alloc_count' "$dir/budget.json"
}

cycles_a="$(frame_cycles "$A")"
cycles_b="$(frame_cycles "$B")"
allocs_a="$(frame_allocs "$A")"
allocs_b="$(frame_allocs "$B")"
span=$((LANES_B - LANES_A))
per_lane_cycles=$(((cycles_b - cycles_a) / span))
per_lane_allocs=$(((allocs_b - allocs_a + span - 1) / span))

echo "per-lane cost, RV32 engine emulator (lp-cli profile steady-render, esp32-c6 cycle model)"
echo "  commit:        $(git rev-parse --short HEAD) (lp-emu last changed at $(git log -1 --format=%h -- lp-emu))"
printf '  %-14s %12s cycles/frame %6s allocs/frame\n' "$(basename "$A")" "$cycles_a" "$allocs_a"
printf '  %-14s %12s cycles/frame %6s allocs/frame\n' "$(basename "$B")" "$cycles_b" "$allocs_b"
echo "  per lane:      ${per_lane_cycles} cycles, ${per_lane_allocs} allocations"
echo "  fixed:         $((cycles_a - LANES_A * per_lane_cycles)) cycles, $((allocs_a - LANES_A * per_lane_allocs)) allocations (+ the LED term)"
