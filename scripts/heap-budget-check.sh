#!/usr/bin/env bash
set -euo pipefail

# Heap-budget ratchet gate.
#
# Measures per-window heap budget figures for each recorded project by running
# `lp-cli profile --collect alloc` on the RV32 emulator in two modes, then
# compares them against the checked-in measured record:
#
#   startup       — project-load through the first compiled frame: every
#                   window (project-load, shader-compile, shader-link, frame).
#                   Its `frame` window contains the shader compile, so it is a
#                   cold-start figure, not a steady-state one.
#   steady-render — 2 warm-up frames, then 4 captured steady frames: only the
#                   `frame` window is recorded. This is where the per-frame
#                   allocation ratchet (alloc_count / alloc_bytes) lives.
#
# Figures per window: transient, retained, largest_alloc (residency) and
# alloc_count, alloc_bytes (requests inside ONE opening, maximised across
# openings — the worst frame). This is a RATCHET, not a ceiling: the record
# holds today's measured values (descriptive), and any growth beyond the
# margin fails. An intentional increase re-baselines explicitly
# (`just heap-budget-baseline`) so the growth lands in the PR diff where a
# reviewer sees it.
#
# Why deltas and not absolutes, and what this gate cannot see:
# docs/heap-budget-gate.md
#
# Usage:
#   heap-budget-check.sh check [margin_pct]   # default margin 0
#   heap-budget-check.sh baseline

cd "$(dirname "$0")/.."

RECORD="scripts/heap-budget-record.json"
MODES=(startup steady-render)
# Windows recorded per mode. Startup records everything the trace has;
# steady-render captures after the compile, so its other windows would only
# record zeros.
STEADY_WINDOWS='["frame"]'
DEFAULT_PROJECTS=(examples/basic examples/meteor)

command -v jq >/dev/null 2>&1 || {
    echo "jq not found. Install it (brew install jq / apt-get install jq) to run the heap-budget gate."
    exit 1
}

# Run one profile session; prints the profile output directory.
run_profile() {
    local project="$1" mode="$2"
    cargo run -q -p lp-cli -- profile "$project" --collect alloc --mode "$mode" 2>/dev/null | tail -1
}

budget_for() {
    local project="$1" mode="$2"
    local dir
    dir="$(run_profile "$project" "$mode")"
    local budget="${dir}/budget.json"
    if [ ! -f "$budget" ]; then
        echo "::error::heap-budget: ${project} (${mode}): no budget.json at ${dir} (was --collect alloc dropped?)" >&2
        exit 1
    fi
    echo "$budget"
}

# The measured windows for one mode, projected to the recorded figures:
# `{windows: {<name>: {transient, retained, largest_alloc, alloc_count, alloc_bytes}}}`.
project_windows() {
    local mode="$1" budget="$2"
    local keep='true'
    [ "$mode" = "steady-render" ] && keep=".name as \$n | ${STEADY_WINDOWS} | index(\$n) != null"
    jq --arg keep "$keep" "
        {windows: (.windows
            | map(select($keep))
            | map({key: .name, value: {transient, retained, largest_alloc, alloc_count, alloc_bytes}})
            | from_entries)}" "$budget"
}

mode="${1:-check}"

case "$mode" in
check)
    margin="${2:-0}"
    if [ ! -f "$RECORD" ]; then
        echo "::error::heap-budget: record ${RECORD} missing — run 'just heap-budget-baseline' and commit it."
        exit 1
    fi
    fail=0
    for project in $(jq -r '.projects | keys[]' "$RECORD"); do
        for pmode in $(jq -r --arg p "$project" '.projects[$p].modes | keys[]' "$RECORD"); do
            echo "heap-budget: profiling ${project} (${pmode})..."
            budget="$(budget_for "$project" "$pmode")"

            # A recorded window absent from the measurement means the
            # instrument or the instrumented path broke — that must fail, not
            # pass silently.
            missing="$(jq -r --arg p "$project" --arg m "$pmode" --slurpfile meas "$budget" \
                '(.projects[$p].modes[$m].windows | keys) - [$meas[0].windows[].name] | .[]' "$RECORD")"
            if [ -n "$missing" ]; then
                while read -r w; do
                    echo "::error::heap-budget: ${project} (${pmode}): recorded window '${w}' missing from measurement"
                done <<<"$missing"
                fail=1
            fi

            # window <TAB> figure <TAB> recorded <TAB> measured
            rows="$(jq -r --arg p "$project" --arg m "$pmode" --slurpfile meas "$budget" '
                .projects[$p].modes[$m].windows | to_entries[] as {key: $w, value: $figs}
                | ($meas[0].windows[] | select(.name == $w)) as $mw
                | ($figs | to_entries[]) as {key: $f, value: $rec}
                | [$w, $f, $rec, $mw[$f]] | @tsv' "$RECORD")"

            while IFS=$'\t' read -r w f rec meas; do
                [ -n "$w" ] || continue
                if [ "$meas" = "null" ] || [ -z "$meas" ]; then
                    echo "::error::heap-budget: ${project} (${pmode}) ${w}.${f}: figure missing from measurement (older lp-cli?)"
                    fail=1
                    continue
                fi
                allowed=$(awk -v r="$rec" -v m="$margin" 'BEGIN { printf "%d", r * (1 + m / 100) }')
                if [ "$meas" -gt "$allowed" ]; then
                    echo "::error::heap-budget: ${project} (${pmode}) ${w}.${f} grew: ${meas} > recorded ${rec} (margin ${margin}%). Intentional? Re-baseline with 'just heap-budget-baseline' in this PR."
                    fail=1
                elif [ "$meas" -lt "$rec" ]; then
                    echo "  improved: ${w}.${f}: ${rec} -> ${meas} (lock it in with 'just heap-budget-baseline')"
                else
                    echo "  ok: ${w}.${f}: ${meas}"
                fi
            done <<<"$rows"

            if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
                {
                    echo "heap-budget \`${project}\` (${pmode}, margin ${margin}%):"
                    echo '```'
                    echo "$rows" | awk -F'\t' '{ printf "%-16s %-13s recorded %8d  measured %8d\n", $1, $2, $3, $4 }'
                    echo '```'
                } >>"$GITHUB_STEP_SUMMARY"
            fi
        done
    done
    exit "$fail"
    ;;

baseline)
    projects=()
    if [ -f "$RECORD" ]; then
        while read -r p; do projects+=("$p"); done < <(jq -r '.projects | keys[]' "$RECORD")
    else
        projects=("${DEFAULT_PROJECTS[@]}")
    fi

    record="$(jq -n \
        --arg date "$(date +%F)" \
        --arg commit "$(git rev-parse --short HEAD)" \
        '{
            comment: "Measured heap-budget record — a ratchet, not a target. These are what the projects cost TODAY on the RV32 emulator, not what any product may use. Per project and profile mode; steady-render records only the frame window. Regenerate with `just heap-budget-baseline`; see docs/heap-budget-gate.md.",
            recorded: $date,
            commit: $commit,
            projects: {}
        }')"

    # ${projects[@]+...}: a record with an empty .projects leaves this array
    # empty, and on bash 3.2 (macOS) a bare "${projects[@]}" would then abort
    # with an unbound-variable error under `set -u` instead of baselining
    # nothing.
    for project in ${projects[@]+"${projects[@]}"}; do
        for pmode in "${MODES[@]}"; do
            echo "heap-budget: baselining ${project} (${pmode})..."
            budget="$(budget_for "$project" "$pmode")"
            windows="$(project_windows "$pmode" "$budget")"
            record="$(jq --arg p "$project" --arg m "$pmode" --argjson w "$windows" \
                '.projects[$p].modes[$m] = $w' <<<"$record")"
        done
    done

    printf '%s\n' "$record" | jq . >"$RECORD"
    echo "heap-budget: wrote ${RECORD}"
    ;;

*)
    echo "usage: $0 check [margin_pct] | baseline" >&2
    exit 2
    ;;
esac
