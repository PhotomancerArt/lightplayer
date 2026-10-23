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

one() {
    local n="$1" c="$out/cut-$1"
    cp "$x/merged.bin" "$c.flash"
    local cut="--ota-cut-after $n"
    [[ -n "${OPS:-}" ]] && cut="--flash-cut $n:calibrated:1,range=$ops_range,then=stop"
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
        first="${loader:-NONE}${first:+ ($first)}"
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
    printf '%s\tcut=%s\tserving=%s\tafter-cut=%s\trecovery-served=%s\tfinal=%s\n' \
        "$verdict" "$n" "${at:-none}" "$first" "${served:-0}" "$final"
}
export -f one
export x y out cli y_build z_flag pre_flags post_flags ops_range OPS chip
printf '%s\n' "$@" | xargs -P "${JOBS:-4}" -I{} bash -c 'one {}' | sort -t= -k2 -n | tee "$out/summary.tsv"
echo "pass: $(grep -c ^PASS "$out/summary.tsv") / $(wc -l < "$out/summary.tsv" | tr -d ' ')"
