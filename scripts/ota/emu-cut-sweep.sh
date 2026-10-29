#!/usr/bin/env bash
# Power cuts by request count during an X -> Y over-the-air update, on the
# emulated C6 (lp-emu:esp32c6:t1). For each cut N:
#
#   1. the chip starts as X's whole merged image (a flash file);
#   2. `lp-cli emu run --rom-up-flash … --ota-offer <Y>/ota --ota-cut-after N`
#      ends the run right after the Nth update request is answered — a power
#      cut, with the flash written back as it stood;
#   3. a recovery run boots that flash and offers Y again, until the host
#      says `done`.
#
# PASS = the recovery run ends `UpToDate` with Y's core and engine running,
# and its first boot was reachable (an engine's hello, or core-only on its
# link). The bytes served after the cut, beside the piece being moved, are
# the resume evidence: a mid-piece cut asks only for the rest.
#
#   scripts/ota/emu-cut-sweep.sh [--chip esp32c6|esp32s3] <x-image> <y-image> <out> <cut>...
#
# <x-image>/<y-image> are `scripts/ota/build-image.sh` outputs. `Z=1` lets
# the board take encoding 1 (the default is raw only, `--ota-no-z`).
# `JOBS` runs that many cuts at once (default 4). Results: <out>/summary.tsv.
#
# `--chip esp32s3` (S3 updates plan, M7 P10) runs the emulated S3
# (`lp-emu:esp32s3:t1`): `lp-cli emu run --chip esp32s3`, whose board always
# reboots on reset (so no `--reboot-on-reset`). The C6 (no flag) runs the
# same commands it did.
#
# `OPS=1` cuts by FLASH OPERATION instead of by request: cut N is the Nth
# program or erase anywhere in `factory` (the boot records, the cores, the
# engine, the progress record), torn per lp-nor-sim's `calibrated` model
# (`--flash-cut N:calibrated:1,range=<factory>,then=stop`), during the
# update; the recovery run is the same. `OPS_RANGE` overrides the range
# (`<off>+<len>`; default the chip's `factory`).
#
# `OPS=lpfs` (the C6) cuts the UPDATE'S PROJECT STAGE instead: the chip
# starts as X holding older projects (`SEED_PROJECTS`, default the crossing's
# format-11 pair, `lp-cli/tests/fixtures/ota-cross-format/v11`, written over
# X's own wire into `<out>/seed.flash` once), and cut N is the Nth program or
# erase in `lpfs` during the update and its project stage — X's own writes,
# engine Y's first boot and the stage's batches (`--flash-cut
# N:calibrated:1,then=stop`, whose default range is the chip's `lpfs`). The
# recovery run is the same (update, then stage). A cut PASSES when the
# recovery ends `UpToDate` with every project upgraded and nothing refused,
# and a verdict run of the recovered chip (`<cut>.fin.*`) says
# `projects: every project is at format …` and decodes, on every pad the
# reference lit, the last frame X lit before the update (`<out>/ref.frames`).
# `<out>/census.txt` holds the uncut run's lpfs op count (the range to sweep,
# 1..count) and its stage's lines; each row says whether its cut fell in the
# update (`in=update`) or after the update ended, in the stage (`in=stage`).
set -uo pipefail

chip=esp32c6
if [[ "${1:-}" == "--chip" ]]; then
    chip="$2"
    shift 2
fi
x="$(cd "$1" && pwd)"
y="$(cd "$2" && pwd)"
mkdir -p "$3"
out="$(cd "$3" && pwd)"
shift 3
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cli="$repo/target/release/lp-cli"
[[ -x "$cli" ]] || { echo "emu-cut-sweep: build lp-cli first: cargo build --release -p lp-cli" >&2; exit 2; }
y_build="$(jq -r '.buildId' "$y/split.json")"
z_flag=--ota-no-z
[[ -n "${Z:-}" ]] && z_flag=
case "$chip" in
    esp32c6)
        pre_flags=
        post_flags=--reboot-on-reset
        ops_range=0x10000+0x370000
        ;;
    esp32s3)
        jq -e '.chip == "esp32s3"' "$x/split.json" "$y/split.json" >/dev/null || {
            echo "emu-cut-sweep: --chip esp32s3 but an image is not an esp32s3 split" >&2
            exit 2
        }
        pre_flags="--chip esp32s3"
        post_flags=
        ops_range=0x10000+0x600000
        ;;
    *)
        echo "emu-cut-sweep: --chip $chip: expected esp32c6 or esp32s3" >&2
        exit 2
        ;;
esac
ops_range="${OPS_RANGE:-$ops_range}"

# The last complete frame decoded on each pad of a `--dump-frames` file, one
# `pad=<hex>` line per pad, ascending.
last_frames() {
    python3 -I -c '
import json, sys
last = {}
for line in open(sys.argv[1]):
    line = line.strip()
    if not line:
        continue
    f = json.loads(line)
    wire = f.get("wire") or f.get("bytes") or ""
    if isinstance(wire, list):
        wire = bytes(wire).hex()
    if f.get("complete", True) and wire:
        last[f.get("pad")] = wire
for pad in sorted(last, key=str):
    print(f"{pad}={last[pad]}")
' "$1"
}
export -f last_frames

start="$x/merged.bin"
if [[ "${OPS:-}" == lpfs ]]; then
    [[ "$chip" == esp32c6 ]] || { echo "emu-cut-sweep: OPS=lpfs is the C6's" >&2; exit 2; }
    projects="${SEED_PROJECTS:-$repo/lp-cli/tests/fixtures/ota-cross-format/v11}"
    start="$out/seed.flash"
    if [[ ! -f "$start" ]]; then
        # X's chip with the projects written over its own wire (plain
        # `filesystem` writes), nothing loaded yet: its next boot loads the
        # first by path.
        requests=()
        while IFS= read -r rel; do
            requests+=(--request "$(jq -nc --arg p "/projects/$rel" --rawfile d "$projects/$rel" \
                '{filesystem: {write: {path: $p, data: $d}}}')")
        done < <(cd "$projects" && find . -type f ! -name '.*' | sed 's|^\./||' | sort)
        last=$(( 1000000 + ${#requests[@]} / 2 - 1 ))
        cp "$x/merged.bin" "$out/seed.tmp"
        "$cli" emu run --rom-up-flash "$out/seed.tmp" --host-link --reboot-on-reset \
            --timeout 60s --wall-timeout 600 --exit-on "M!{\"id\":$last," "${requests[@]}" \
            >/dev/null 2>"$out/seed.err" || { echo "emu-cut-sweep: seeding failed: $out/seed.err" >&2; exit 1; }
        mv "$out/seed.tmp" "$start"
    fi
    # What X lights with them: the reference every verdict compares against.
    if [[ ! -s "$out/ref.frames" ]]; then
        cp "$start" "$out/ref.flash"
        "$cli" emu run --rom-up-flash "$out/ref.flash" --host-link --reboot-on-reset \
            --timeout 10s --wall-timeout 600 --dump-frames "$out/ref.frames.json" \
            --console "$out/ref.txt" >/dev/null 2>"$out/ref.err"
        last_frames "$out/ref.frames.json" > "$out/ref.frames"
        rm -f "$out/ref.flash"
    fi
    [[ -s "$out/ref.frames" ]] || { echo "emu-cut-sweep: X lit nothing with the seeded projects ($out/ref.txt)" >&2; exit 1; }
    # The census: the uncut update's lpfs ops (the sweep's range is 1..it).
    if [[ ! -s "$out/census.txt" ]]; then
        cp "$start" "$out/census.flash"
        "$cli" emu run --rom-up-flash "$out/census.flash" --host-link --reboot-on-reset \
            --ota-offer "$y/ota" $z_flag --flash-cut 1000000:calibrated:1,then=stop \
            --exit-on "[host-ota] done" --timeout 300s --wall-timeout 1200 \
            --console "$out/census.con" >/dev/null 2>"$out/census.err"
        grep -ao "[0-9]* in-range op(s) counted" "$out/census.err" | head -1 > "$out/census.txt"
        grep -a "\[host-ota\] projects:" "$out/census.con" >> "$out/census.txt"
        rm -f "$out/census.flash"
    fi
    echo "emu-cut-sweep: lpfs census: $(tr '\n' ' ' < "$out/census.txt")" >&2
fi
export start

one() {
    local n="$1" c="$out/cut-$1"
    cp "$start" "$c.flash"
    local cut="--ota-cut-after $n"
    if [[ "${OPS:-}" == lpfs ]]; then
        cut="--flash-cut $n:calibrated:1,then=stop"
    elif [[ -n "${OPS:-}" ]]; then
        cut="--flash-cut $n:calibrated:1,range=$ops_range,then=stop"
    fi
    "$cli" emu run $pre_flags --rom-up-flash "$c.flash" --host-link $post_flags \
        --ota-offer "$y/ota" $z_flag $cut --timeout 300s --wall-timeout 1200 \
        --console "$c.cut.txt" >/dev/null 2>"$c.cut.err"
    "$cli" emu run $pre_flags --rom-up-flash "$c.flash" --host-link $post_flags \
        --ota-offer "$y/ota" $z_flag --exit-on "[host-ota] done" --timeout 300s --wall-timeout 1200 \
        --console "$c.rec.txt" >/dev/null 2>"$c.rec.err"
    local at first served final verdict
    # Where the cut landed: the last piece the host was serving.
    at=$(grep -ao "stage [A-Za-z]*" "$c.cut.txt" | tail -1 | cut -d' ' -f2)
    # The first boot after the cut: its core line, and how it was reached.
    first=$(grep -a -m1 -o "(\(proven\|trial\|rolled back\|fallback\)) build [^ ]*" "$c.rec.txt" || echo "NONE")
    if grep -aq '"hello":{' "$c.rec.txt" || grep -aq "\[OTA\] core-only:" "$c.rec.txt"; then
        first="$first reachable"
    else
        first="$first NOT-REACHABLE"
    fi
    served=$(grep -ao "D [0-9]* chunk(s) / [0-9]* B; Z [0-9]* chunk(s) / [0-9]* B" "$c.rec.err" | tr -d '\n')
    if [[ "$chip" == esp32s3 ]]; then
        # The S3's `[CORE]` line rides the log channel and can be lost when
        # the first link session after a reset is reset
        # (docs/defects/2026-10-22-the-s3-loses-its-core-line-after-an-updates-reset.md):
        # the boot is the loader's line (raw console), reachability the
        # board answering on its link (an engine's hello, or a manifest the
        # host recorded), Y the newest hello's firmware since the last boot.
        loader=$(grep -a -m1 -o "\[LOADER\] core @0x[0-9a-f]* ([a-z ]*)" "$c.rec.txt" | cut -c10- || true)
        core_line=$(grep -a -m1 -o "(\(proven\|trial\|rolled back\|fallback\)) build [^ ]*" "$c.rec.txt" || true)
        first="${loader:-NONE}${core_line:+ [$core_line]}"
        if grep -aq '"hello":{' "$c.rec.txt" || grep -aq "\[host-ota\] board:" "$c.rec.txt"; then
            first="$first reachable"
        else
            first="$first NOT-REACHABLE"
        fi
        # Y's hello can come after the host's `done` (the board holds it
        # until its core's hash is known), so the verdict boots the written-
        # back flash once more, power-on, to Y's hello: Y's engine runs.
        "$cli" emu run $pre_flags --rom-up-flash "$c.flash" --host-link $post_flags \
            --exit-on "\"buildId\":\"$y_build\"" --timeout 300s --wall-timeout 1200 \
            --console "$c.fin.txt" >/dev/null 2>"$c.fin.err"
        final=$(grep -a '"hello":{' "$c.fin.txt" | tail -1 \
            | grep -o '"buildId":"[^"]*"\|"state":"[a-z-]*"' | tr '\n' ' ')
        if grep -aq "done: UpToDate" "$c.rec.txt" && [[ "$final" == *"\"buildId\":\"$y_build\""*"\"state\":\"running\""* ]]; then
            verdict=PASS
        else
            verdict=FAIL
        fi
    else
        final=$(grep -a "\[CORE\] core @" "$c.rec.txt" | tail -1 | grep -o "build [^ ]*\|engine [0-9]* B" | tr '\n' ' ')
        if grep -aq "done: UpToDate" "$c.rec.txt" && [[ "$final" == *"build $y_build"*"engine "* ]]; then
            verdict=PASS
        else
            verdict=FAIL
        fi
    fi
    if [[ "${OPS:-}" == lpfs ]]; then
        # Where the cut fell: after the update ended (in its stage) or before.
        if grep -aq "done: UpToDate" "$c.cut.txt"; then at="stage"; else at="update"; fi
        # The recovery's stage: every project it found older, upgraded.
        local stage
        stage=$(grep -a "\[host-ota\] projects:" "$c.rec.txt" | sed 's/.*projects: //' | tr '\n' ';')
        if grep -aq "\[host-ota\] projects: \(couldn't\|.*still needs\|.*didn't start\|.*needs upgrading\|the board never\|the link went\)" "$c.rec.txt" \
            || ! grep -aq "\[host-ota\] projects:" "$c.rec.txt"; then
            verdict=FAIL
        fi
        # The verdict run: the recovered chip, Y offered again, to its
        # deadline: nothing older left, and X's frames on every pad.
        "$cli" emu run $pre_flags --rom-up-flash "$c.flash" --host-link $post_flags \
            --ota-offer "$y/ota" $z_flag --timeout 12s --wall-timeout 1200 \
            --dump-frames "$c.fin.frames.json" --console "$c.fin.txt" >/dev/null 2>"$c.fin.err"
        last_frames "$c.fin.frames.json" > "$c.fin.frames"
        local frames=equal
        while IFS= read -r ref; do
            grep -qxF "$ref" "$c.fin.frames" || frames=DIFFER
        done < "$out/ref.frames"
        grep -aq "\[host-ota\] projects: every project is at format" "$c.fin.txt" || verdict=FAIL
        [[ "$frames" == equal ]] || verdict=FAIL
        final="$final; stage: ${stage:-none}; after: $(grep -a "\[host-ota\] projects:" "$c.fin.txt" | sed 's/.*projects: //' | tr '\n' ' ')frames $frames"
        [[ -n "${DROP_FLASH:-}" ]] && rm -f "$c.fin.frames.json"
    fi
    # `DROP_FLASH=1`: delete the cut's chip copy once judged (8 MB a cut on
    # the S3; a long sweep's are gigabytes).
    [[ -n "${DROP_FLASH:-}" ]] && rm -f "$c.flash"
    printf '%s\tcut=%s\t%s=%s\tafter-cut=%s\trecovery-served=%s\tfinal=%s\n' \
        "$verdict" "$n" "$([[ "${OPS:-}" == lpfs ]] && echo in || echo serving)" "${at:-none}" "$first" "${served:-0}" "$final"
}
export -f one
export x y out cli y_build z_flag pre_flags post_flags ops_range OPS chip DROP_FLASH start
printf '%s\n' "$@" | xargs -P "${JOBS:-4}" -I{} bash -c 'one {}' | sort -t= -k2 -n | tee "$out/summary.tsv"
echo "pass: $(grep -c ^PASS "$out/summary.tsv") / $(wc -l < "$out/summary.tsv" | tr -d ' ')"
