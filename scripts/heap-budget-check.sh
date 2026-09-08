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
# openings — the worst frame), plus largest_free_at_close and holes_at_close
# (the guest's own free-list shape at the window's close, MIN/MAX across
# openings — see docs/heap-budget-gate.md). Every figure but
# largest_free_at_close is a RATCHET on GROWTH: the record holds today's
# measured values (descriptive), and any growth beyond the margin fails.
# largest_free_at_close inverts that: it fails when the measurement SHRANK
# below the record, because a bigger free block is the good direction there.
# An intentional change re-baselines explicitly (`just heap-budget-baseline`)
# so it lands in the PR diff where a reviewer sees it.
#
# Why deltas and not absolutes, and what this gate cannot see:
# docs/heap-budget-gate.md
#
# ## The second source: a whole chip, not an engine
#
# Everything above is the RV32 **engine** emulator — the product's own
# `lp-cli profile`, which runs the render engine on a host and knows nothing
# about an ESP32. It measures what a project costs; it cannot measure what the
# firmware costs, because in that emulator there is no firmware.
#
# `scripts/heap-budget-record.json`'s `chips` section is the other half, and
# it comes from the SoC emulator (`lp-emu-esp32c6`, plan
# `2026-09-06-1001-esp-emulator`): the shipped image booted whole, and the
# `allocator` figures its own first heartbeat reports. That is the figure a
# board reports, from the bytes a board is flashed with — M3 through M7
# established it byte-equal to silicon on every memory-class value but a
# constant 8 B, which the record carries beside it rather than hiding.
#
# The chip half needs a firmware ELF, so it does **not** run in the required
# `test-rust-core` job: the cost rule that keeps a cross-target build out of
# every workspace test run applies here too (`lp-emu-esp32c6`'s
# `test_support` module says why). It runs in the path-gated `emu-c6` job,
# which builds firmware already. Without an image the chip half prints a
# named SKIP and the projects half still gates; in `emu-c6`, where
# `LP_EMU_BUILD_FW=1` is set, a skip is a failure.
#
# Usage:
#   heap-budget-check.sh check [margin_pct]   # default margin 0
#   heap-budget-check.sh baseline
#   heap-budget-check.sh chips [margin_pct]   # the chip half alone
#   heap-budget-check.sh chips-baseline

cd "$(dirname "$0")/.."

RECORD="scripts/heap-budget-record.json"
MODES=(startup steady-render)
# Emulator cycle cap per profile session. The startup capture must reach the
# END of the frame that contains the first shader compile; a capture the cap
# cuts short leaves that window OPEN, and an open window's figures are an
# artifact of where the cap fell (meteor's compile begins ~165M cycles in
# and needs well past 200M — docs/defects/2026-09-06-heap-budget-capture-
# truncated-by-cycle-cap.md). `budget_for` refuses a truncated capture rather
# than comparing or recording its numbers.
MAX_CYCLES=400000000
# Windows recorded per mode. Startup records everything the trace has;
# steady-render captures after the compile, so its other windows would only
# record zeros.
STEADY_WINDOWS='["frame"]'
DEFAULT_PROJECTS=(projects/test/basic catalog/patterns/meteor)

command -v jq >/dev/null 2>&1 || {
    echo "jq not found. Install it (brew install jq / apt-get install jq) to run the heap-budget gate."
    exit 1
}

# Run one profile session; prints the profile output directory.
run_profile() {
    local project="$1" mode="$2"
    # The safety cap is raised over lp-cli's 200 M default: the per-marker
    # free-list walk grows with the guest's free heap, and a startup run of
    # zook-dome crossed 200 M cycles mid-walk on 2026-09-06 (after the sample
    # window change freed ~21 KB), which silently drops the last window's
    # free-list figures. See docs/heap-budget-gate.md "Cost". Meteor's startup
    # capture never fit the default at all (`MAX_CYCLES` above); a session
    # that still hits the cap is refused by `budget_for`.
    cargo run -q -p lp-cli -- profile "$project" --collect alloc --mode "$mode" \
        --max-cycles "$MAX_CYCLES" 2>/dev/null | tail -1
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
    # A capture the cycle cap ended is not a measurement: the window it cut
    # is still open and its figures depend on where the cap fell, not on
    # what the window costs. Refuse it in both `check` and `baseline`.
    local terminated_by
    terminated_by="$(jq -r '.terminated_by // empty' "${dir}/meta.json" 2>/dev/null || true)"
    if [ "$terminated_by" = "max_cycles" ]; then
        echo "::error::heap-budget: ${project} (${mode}): capture hit --max-cycles ${MAX_CYCLES} before the mode's gate closed (${dir}); its figures are truncation artifacts. Raise MAX_CYCLES in scripts/heap-budget-check.sh." >&2
        exit 1
    fi
    echo "$budget"
}

# The measured windows for one mode, projected to the recorded figures:
# `{windows: {<name>: {transient, retained, largest_alloc, alloc_count,
# alloc_bytes, largest_free_at_close?, holes_at_close?}}}`. The last two are
# absent from a measurement with no guest free-list-shape rows (older
# lp-cli), and the projection preserves that absence rather than writing a
# `null` into the record — see docs/heap-budget-gate.md.
project_windows() {
    local mode="$1" budget="$2"
    local keep='true'
    [ "$mode" = "steady-render" ] && keep=".name as \$n | ${STEADY_WINDOWS} | index(\$n) != null"
    jq --arg keep "$keep" "
        {windows: (.windows
            | map(select($keep))
            | map({key: .name, value: (
                {transient, retained, largest_alloc, alloc_count, alloc_bytes}
                + (if has(\"largest_free_at_close\") then {largest_free_at_close} else {} end)
                + (if has(\"holes_at_close\") then {holes_at_close} else {} end)
              )})
            | from_entries)}" "$budget"
}

# ---------------------------------------------------------------- the chips
#
# One chip today. The C6's shipped feature set, booted on `lp-emu:esp32c6:t1`
# until its first heartbeat, whose `allocator` object and `[stack]` line are
# what a board prints over the same link.
CHIP_ID="esp32c6"
CHIP_FEATURES="esp32c6,server,radio"
CHIP_SLUG="ESP32C6_SERVER_RADIO"
# Emulated microseconds. The heartbeat is on a 5 s tick; 6.5 s reaches the
# first one with room and stops well before the second, so the figures are
# always the SAME sample (M6 P4's finding: keying on "a heartbeat" rather than
# on the 5 s tick let last-writes pick whichever one a capture ended on).
CHIP_TIMEOUT="6500ms"
# Direct load, not ROM-up. M7's G7-4 measured the two paths' idle heap
# byte-identical, and this gate runs on every emulator PR — the bootloader
# adds seconds of wall clock and nothing to the answer. The walk
# (`scripts/emu/m4-walk.sh`) is the place that boots the whole chain.

# The shipped ELF, by the emulator's own resolution rules (its `test_support`
# module is the authority): an explicit path, then the copy an earlier build
# left, then — only with `LP_EMU_BUILD_FW=1` — a build. Prints the path, or
# nothing and a reason on stderr.
chip_elf() {
    if [ -n "${LP_EMU_C6_ELF_ESP32C6_SERVER_RADIO:-}" ]; then
        if [ -f "$LP_EMU_C6_ELF_ESP32C6_SERVER_RADIO" ]; then
            echo "$LP_EMU_C6_ELF_ESP32C6_SERVER_RADIO"
            return 0
        fi
        echo "LP_EMU_C6_ELF_${CHIP_SLUG} points at a file that is not there" >&2
        return 1
    fi
    # The emulator's per-source-tree copy. Keyed by the tree, so a stale ELF
    # is a miss rather than a wrong answer — the same reason `test_support`
    # refuses `target/<triple>/release-esp32/fw-esp32c6`, which is where every
    # feature set of that crate builds to and therefore holds whatever was
    # built last.
    local cached
    cached="$(ls -1t target/lp-emu-c6/${CHIP_SLUG}-*/fw-esp32c6 2>/dev/null | head -1 || true)"
    if [ -n "$cached" ] && [ -f "$cached" ]; then
        echo "$cached"
        return 0
    fi
    if [ "${LP_EMU_BUILD_FW:-}" != "1" ]; then
        echo "no fw-esp32c6 ELF for ${CHIP_FEATURES}. Set LP_EMU_C6_ELF_${CHIP_SLUG} to one, or \
LP_EMU_BUILD_FW=1 to build it. Not built automatically: a workspace gate must not start a \
cross-target firmware build." >&2
        return 1
    fi
    ( cd lp-fw/fw-esp32c6 && cargo build --quiet --target riscv32imac-unknown-none-elf \
        --profile release-esp32 --features "$CHIP_FEATURES" ) >&2 || return 1
    echo "target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6"
}

# `{freeBytes, usedBytes, totalBytes, largestFreeBlock, stackHighWater,
# stackTotal}` from the first heartbeat of a boot, as JSON on stdout.
chip_measure() {
    local elf="$1" dir
    dir="$(mktemp -d "${TMPDIR:-/tmp}/heap-budget-chip.XXXXXX")"
    # No `--link`: nothing needs to talk to it, and a gate that binds a port
    # collides with whatever is already using one.
    if ! cargo run -q -p lp-cli -- emu run --elf "$elf" \
            --timeout "$CHIP_TIMEOUT" --console "$dir/console.txt" \
            >"$dir/emu.out" 2>"$dir/emu.err"; then
        echo "::error::heap-budget: ${CHIP_ID}: the boot did not end cleanly" >&2
        tail -20 "$dir/emu.err" >&2
        rm -rf "$dir"
        return 1
    fi
    local memory stack
    memory="$(grep -ao '"memory":{[^}]*}' "$dir/console.txt" | head -1 || true)"
    stack="$(grep -ao '\[stack\] heartbeat: high-water [0-9]* B of [0-9]* B' "$dir/console.txt" \
        | head -1 || true)"
    if [ -z "$memory" ] || [ -z "$stack" ]; then
        echo "::error::heap-budget: ${CHIP_ID}: no first heartbeat in ${CHIP_TIMEOUT} of emulated \
time (memory line: ${memory:-none}; stack line: ${stack:-none}). The console is at \
$dir/console.txt." >&2
        return 1
    fi
    local high total
    high="$(echo "$stack" | awk '{print $4}')"
    total="$(echo "$stack" | awk '{print $7}')"
    echo "{${memory#\"memory\":\{}" \
        | jq --argjson h "$high" --argjson t "$total" \
             '{freeBytes, usedBytes, totalBytes, largestFreeBlock,
               stackHighWater: $h, stackTotal: $t}'
    rm -rf "$dir"
}

# The four memory figures are exact or ratcheted; the stack high-water is a
# BAND. DD45/DD46 of the plan: the reference image a CI runner builds is not
# the binary this host builds (same nightly, different rustc `.text`), the
# code lands at different addresses, and a tick that lands on a different
# instruction of a differently laid-out image has a different deepest point.
# The memory class survives that; the stack figure does not, and pretending
# otherwise would be a gate that fails on the host it runs on.
chip_check() {
    local margin="$1" fail=0
    local elf
    if ! elf="$(chip_elf)"; then
        echo "heap-budget: ${CHIP_ID}: SKIPPED — no firmware image (see stderr). The chip half \
runs in CI's path-gated 'emu-c6' job, which sets LP_EMU_BUILD_FW=1."
        return 0
    fi
    echo "heap-budget: booting ${CHIP_ID} (${CHIP_FEATURES}) on lp-emu:esp32c6:t1 — $elf"
    local meas
    meas="$(chip_measure "$elf")" || return 1

    local recorded
    recorded="$(jq -c --arg c "$CHIP_ID" '.chips[$c].measured // empty' "$RECORD")"
    if [ -z "$recorded" ]; then
        echo "::error::heap-budget: ${CHIP_ID}: no .chips.${CHIP_ID}.measured in ${RECORD} — run \
'heap-budget-check.sh chips-baseline' and commit it."
        return 1
    fi

    # figure <TAB> recorded <TAB> measured <TAB> direction
    #   grow  — bigger is worse (usedBytes)
    #   shrink— smaller is worse (freeBytes, largestFreeBlock)
    #   exact — any difference is a finding (totalBytes, stackTotal)
    #   band  — inside the recorded range (stackHighWater)
    local rows
    rows="$(jq -rn --argjson rec "$recorded" --argjson m "$meas" '
        [ ["totalBytes", "exact"], ["usedBytes", "grow"], ["freeBytes", "shrink"],
          ["largestFreeBlock", "shrink"], ["stackTotal", "exact"] ][]
        | . as [$f, $dir] | [$f, ($rec[$f] // "null"), ($m[$f] // "null"), $dir] | @tsv')"
    while IFS=$'\t' read -r f rec meas_v dir; do
        [ -n "$f" ] || continue
        if [ "$rec" = "null" ] || [ "$meas_v" = "null" ]; then
            echo "::error::heap-budget: ${CHIP_ID} ${f}: missing from the record or the measurement"
            fail=1
            continue
        fi
        case "$dir" in
        exact)
            if [ "$meas_v" != "$rec" ]; then
                echo "::error::heap-budget: ${CHIP_ID} ${f} changed: ${meas_v} != recorded ${rec}. \
This figure is the chip's, not a budget — a change is a finding. Intentional? Re-baseline with \
'just heap-budget-baseline-chips' in this PR."
                fail=1
            else
                echo "  ok: ${f}: ${meas_v}"
            fi
            ;;
        grow)
            allowed=$(awk -v r="$rec" -v m="$margin" 'BEGIN { printf "%d", r * (1 + m / 100) }')
            if [ "$meas_v" -gt "$allowed" ]; then
                echo "::error::heap-budget: ${CHIP_ID} ${f} grew: ${meas_v} > recorded ${rec} \
(margin ${margin}%). The firmware's own resident cost went up. Intentional? Re-baseline with \
'just heap-budget-baseline-chips' in this PR."
                fail=1
            elif [ "$meas_v" -lt "$rec" ]; then
                echo "  improved: ${f}: ${rec} -> ${meas_v} (lock it in with 'just heap-budget-baseline-chips')"
            else
                echo "  ok: ${f}: ${meas_v}"
            fi
            ;;
        shrink)
            allowed=$(awk -v r="$rec" -v m="$margin" 'BEGIN { printf "%d", r * (1 - m / 100) }')
            if [ "$meas_v" -lt "$allowed" ]; then
                echo "::error::heap-budget: ${CHIP_ID} ${f} shrank: ${meas_v} < recorded ${rec} \
(margin ${margin}%). Intentional? Re-baseline with 'just heap-budget-baseline-chips' in this PR."
                fail=1
            elif [ "$meas_v" -gt "$rec" ]; then
                echo "  improved: ${f}: ${rec} -> ${meas_v} (lock it in with 'just heap-budget-baseline-chips')"
            else
                echo "  ok: ${f}: ${meas_v}"
            fi
            ;;
        esac
    done <<<"$rows"

    # The band.
    local hw lo hi
    hw="$(jq -r '.stackHighWater' <<<"$meas")"
    lo="$(jq -r --arg c "$CHIP_ID" '.chips[$c].measured.stackHighWaterBand[0]' "$RECORD")"
    hi="$(jq -r --arg c "$CHIP_ID" '.chips[$c].measured.stackHighWaterBand[1]' "$RECORD")"
    if [ "$hw" -lt "$lo" ] || [ "$hw" -gt "$hi" ]; then
        echo "::error::heap-budget: ${CHIP_ID} stackHighWater ${hw} B is outside the recorded band \
${lo}..${hi} B. The band is wide on purpose (the image a CI runner builds is not this host's \
binary — see docs/heap-budget-gate.md); leaving it is a real change in how deep an interrupt \
lands on the main task."
        fail=1
    else
        echo "  ok: stackHighWater: ${hw} B (band ${lo}..${hi})"
    fi

    # Silicon, beside it. Never gated — it is a different image at a different
    # commit — but printed every run, because the whole claim of this source is
    # that the two agree, and a gap nobody looks at is a gap nobody notices
    # widening.
    local sil
    sil="$(jq -c --arg c "$CHIP_ID" '.chips[$c].silicon_reference // empty' "$RECORD")"
    if [ -n "$sil" ]; then
        echo "  silicon reference ($(jq -r '.commit' <<<"$sil"), $(jq -r '.transcript' <<<"$sil")):"
        jq -rn --argjson s "$sil" --argjson m "$meas" '
            ["freeBytes", "usedBytes", "totalBytes", "largestFreeBlock"][]
            | . as $f | "    \($f): silicon \($s.figures[$f]) / this tree \($m[$f] // "?")"'
        echo "    the pinned gap is $(jq -r '.gap_note' <<<"$sil")"
    fi

    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        {
            echo "heap-budget \`${CHIP_ID}\` (${CHIP_FEATURES}, lp-emu:esp32c6:t1):"
            echo '```'
            jq -r 'to_entries[] | "\(.key): \(.value)"' <<<"$meas"
            echo '```'
        } >>"$GITHUB_STEP_SUMMARY"
    fi
    return "$fail"
}

chip_baseline() {
    local elf
    elf="$(chip_elf)" || {
        echo "heap-budget: cannot baseline ${CHIP_ID} without a firmware image." >&2
        return 1
    }
    echo "heap-budget: baselining ${CHIP_ID} (${CHIP_FEATURES}) — $elf"
    local meas
    meas="$(chip_measure "$elf")" || return 1
    # The band is the measurement ±500 B, rounded outward to the nearest 100:
    # DD45's measured spread between this host's image and a CI runner's was
    # 128 B, and the band is the documented spread with room, never a fitted
    # number.
    local hw lo hi
    hw="$(jq -r '.stackHighWater' <<<"$meas")"
    lo=$(( (hw - 500) / 100 * 100 ))
    hi=$(( (hw + 599) / 100 * 100 ))
    local updated
    updated="$(jq --arg c "$CHIP_ID" --argjson m "$meas" --argjson lo "$lo" --argjson hi "$hi" \
        --arg commit "$(git rev-parse --short HEAD)" --arg date "$(date +%F)" '
        .chips[$c].measured = ($m + {stackHighWaterBand: [$lo, $hi]})
        | .chips[$c].recorded = $date
        | .chips[$c].commit = $commit' "$RECORD")"
    printf '%s\n' "$updated" | jq . >"$RECORD"
    echo "heap-budget: wrote ${CHIP_ID} into ${RECORD}"
}

mode="${1:-check}"

case "$mode" in
chips)
    chip_check "${2:-0}"
    exit $?
    ;;

chips-baseline)
    chip_baseline
    exit $?
    ;;

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
                if [ "$f" = "largest_free_at_close" ]; then
                    # Inverted direction: a bigger largest-free-block is the
                    # improvement here, so the ratchet fails on SHRINKING
                    # below the record, not on growth.
                    allowed=$(awk -v r="$rec" -v m="$margin" 'BEGIN { printf "%d", r * (1 - m / 100) }')
                    if [ "$meas" -lt "$allowed" ]; then
                        echo "::error::heap-budget: ${project} (${pmode}) ${w}.${f} shrank: ${meas} < recorded ${rec} (margin ${margin}%). Intentional? Re-baseline with 'just heap-budget-baseline' in this PR."
                        fail=1
                    elif [ "$meas" -gt "$rec" ]; then
                        echo "  improved: ${w}.${f}: ${rec} -> ${meas} (lock it in with 'just heap-budget-baseline')"
                    else
                        echo "  ok: ${w}.${f}: ${meas}"
                    fi
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
    # The chip half, which skips with a named reason when there is no firmware
    # image. `heap-budget-check.sh chips` is the same call on its own, for the
    # job that HAS one and must not skip.
    chip_check "$margin" || fail=1
    exit "$fail"
    ;;

baseline)
    projects=()
    if [ -f "$RECORD" ]; then
        while read -r p; do projects+=("$p"); done < <(jq -r '.projects | keys[]' "$RECORD")
    else
        projects=("${DEFAULT_PROJECTS[@]}")
    fi

    # The `chips` section is carried through untouched: it comes from a
    # different emulator, needs a firmware build, and is baselined by
    # `chips-baseline`. A wholesale regeneration that silently dropped it
    # would delete a gate.
    chips="$(jq -c '.chips // {}' "$RECORD" 2>/dev/null || echo '{}')"
    record="$(jq -n \
        --arg date "$(date +%F)" \
        --arg commit "$(git rev-parse --short HEAD)" \
        --argjson chips "$chips" \
        '{
            comment: "Measured heap-budget record — a ratchet, not a target. Two sources. `projects` is what each project costs TODAY on the RV32 ENGINE emulator (`lp-cli profile`), per project and profile mode; steady-render records only the frame window. `chips` is what the FIRMWARE costs on the SoC emulator (`lp-emu-esp32c6`) — the shipped image booted whole, read from its own first heartbeat, with silicon beside it. Regenerate with `just heap-budget-baseline` and `just heap-budget-baseline-chips`; see docs/heap-budget-gate.md.",
            recorded: $date,
            commit: $commit,
            projects: {},
            chips: $chips
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
    echo "usage: $0 check [margin_pct] | baseline | chips [margin_pct] | chips-baseline" >&2
    exit 2
    ;;
esac
