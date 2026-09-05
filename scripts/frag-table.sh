#!/usr/bin/env bash
set -euo pipefail

# Counterfactual fragmentation tables for the three reference projects.
#
# Runs `lp-cli profile --collect alloc --mode startup` per project with the
# `studio-sync` workload (so the trace carries a `project-read` window), the two
# emulator-only discounts, and every counterfactual, then prints each run's
# `Heap Counterfactuals` section. P6 pastes from this output.
#
# The discounts are not optional: `fw-emu` runs a 256-resource board manifest,
# so `VirtualWs281xDriver::endpoints` and the manifest's `Vec<HwResource>`
# allocate amounts no firmware ever will and would dominate every row. See
# docs/heap-budget-gate.md, "Discounting emulator-only artifacts".
#
# ⚠️ `--mode startup` stops at the frame that contained the first shader
# compile, and the staged reads are served in those same ticks — so what lands
# in the `project-read` window is Studio's skeleton read, not the whole staged
# sync. That is stated in the output rather than worked around: a longer run
# (`--mode all`) measures a different workload.
#
# Usage:
#   scripts/frag-table.sh [project ...]     # default: the three reference projects

cd "$(dirname "$0")/.."

PROJECTS=("$@")
if [ ${#PROJECTS[@]} -eq 0 ]; then
    PROJECTS=(examples/basic examples/meteor examples/zook-dome)
fi

DISCOUNTS=(
    --frag-discount-site VirtualWs281xDriver::endpoints
    --frag-discount-site HwResource
)

COUNTERFACTUALS=(
    --cf "scratch=shader-compile,project-read"
    --cf "residents-first=project-load,frame"
    --cf "tlsf"
    --cf "scratch=shader-compile,project-read+residents-first=project-load,frame"
)

# Run one profile session; prints the profile output directory. Same shape as
# `run_profile` in scripts/heap-budget-check.sh, plus this table's flags.
run_profile() {
    local project="$1"
    cargo run -q -p lp-cli -- profile "$project" \
        --collect alloc --mode startup --workload studio-sync \
        "${DISCOUNTS[@]}" "${COUNTERFACTUALS[@]}" 2>/dev/null | tail -1
}

for project in "${PROJECTS[@]}"; do
    echo "frag-table: profiling ${project} (startup, studio-sync)..." >&2
    dir="$(run_profile "$project")"
    report="${dir}/report.txt"
    if [ ! -f "$report" ]; then
        echo "::error::frag-table: ${project}: no report.txt at ${dir} (did the run fail?)" >&2
        exit 1
    fi
    echo
    echo "## ${project} — startup, classic layout, discounted"
    echo "(profile dir: ${dir})"
    echo
    sed -n '/=== Heap Counterfactuals ===/,$p' "$report"
done
