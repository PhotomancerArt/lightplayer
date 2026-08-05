#!/usr/bin/env bash
set -euo pipefail

# Heap-budget ratchet gate.
#
# Measures per-window heap budget figures (transient / retained / largest
# single allocation) for each recorded project by running `lp-cli profile
# --collect alloc` on the RV32 emulator, then compares them against the
# checked-in measured record. This is a RATCHET, not a ceiling: the record
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
PROFILE_FLAGS=(--collect alloc --mode startup)
DEFAULT_PROJECTS=(examples/basic)

command -v jq >/dev/null 2>&1 || {
    echo "jq not found. Install it (brew install jq / apt-get install jq) to run the heap-budget gate."
    exit 1
}

# Run one profile session; prints the profile output directory.
run_profile() {
    local project="$1"
    cargo run -q -p lp-cli -- profile "$project" "${PROFILE_FLAGS[@]}" 2>/dev/null | tail -1
}

budget_for() {
    local project="$1"
    local dir
    dir="$(run_profile "$project")"
    local budget="${dir}/budget.json"
    if [ ! -f "$budget" ]; then
        echo "::error::heap-budget: ${project}: no budget.json at ${dir} (was --collect alloc dropped?)" >&2
        exit 1
    fi
    echo "$budget"
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
        echo "heap-budget: profiling ${project}..."
        budget="$(budget_for "$project")"

        # A recorded window absent from the measurement means the instrument
        # or the instrumented path broke — that must fail, not pass silently.
        missing="$(jq -r --arg p "$project" --slurpfile m "$budget" \
            '(.projects[$p].windows | keys) - [$m[0].windows[].name] | .[]' "$RECORD")"
        if [ -n "$missing" ]; then
            while read -r w; do
                echo "::error::heap-budget: ${project}: recorded window '${w}' missing from measurement"
            done <<<"$missing"
            fail=1
        fi

        # window <TAB> figure <TAB> recorded <TAB> measured
        rows="$(jq -r --arg p "$project" --slurpfile m "$budget" '
            .projects[$p].windows | to_entries[] as {key: $w, value: $figs}
            | ($m[0].windows[] | select(.name == $w)) as $meas
            | ($figs | to_entries[]) as {key: $f, value: $rec}
            | [$w, $f, $rec, $meas[$f]] | @tsv' "$RECORD")"

        while IFS=$'\t' read -r w f rec meas; do
            [ -n "$w" ] || continue
            allowed=$(awk -v r="$rec" -v m="$margin" 'BEGIN { printf "%d", r * (1 + m / 100) }')
            if [ "$meas" -gt "$allowed" ]; then
                echo "::error::heap-budget: ${project} ${w}.${f} grew: ${meas} B > recorded ${rec} B (margin ${margin}%). Intentional? Re-baseline with 'just heap-budget-baseline' in this PR."
                fail=1
            elif [ "$meas" -lt "$rec" ]; then
                echo "  improved: ${w}.${f}: ${rec} -> ${meas} B (lock it in with 'just heap-budget-baseline')"
            else
                echo "  ok: ${w}.${f}: ${meas} B"
            fi
        done <<<"$rows"

        if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
            {
                echo "heap-budget \`${project}\` (margin ${margin}%):"
                echo '```'
                echo "$rows" | awk -F'\t' '{ printf "%-16s %-13s recorded %8d B  measured %8d B\n", $1, $2, $3, $4 }'
                echo '```'
            } >>"$GITHUB_STEP_SUMMARY"
        fi
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
            comment: "Measured heap-budget record — a ratchet, not a target. These are what the projects cost TODAY on the RV32 emulator, not what any product may use. Regenerate with `just heap-budget-baseline`; see docs/heap-budget-gate.md.",
            recorded: $date,
            commit: $commit,
            mode: "startup",
            projects: {}
        }')"

    # ${projects[@]+...}: a record with an empty .projects leaves this array
    # empty, and on bash 3.2 (macOS) a bare "${projects[@]}" would then abort
    # with an unbound-variable error under `set -u` instead of baselining
    # nothing.
    for project in ${projects[@]+"${projects[@]}"}; do
        echo "heap-budget: baselining ${project}..."
        budget="$(budget_for "$project")"
        windows="$(jq '{windows: (.windows | map({key: .name, value: {transient, retained, largest_alloc}}) | from_entries)}' "$budget")"
        record="$(jq --arg p "$project" --argjson w "$windows" '.projects[$p] = $w' <<<"$record")"
    done

    printf '%s\n' "$record" | jq . >"$RECORD"
    echo "heap-budget: wrote ${RECORD}"
    ;;

*)
    echo "usage: $0 check [margin_pct] | baseline" >&2
    exit 2
    ;;
esac
