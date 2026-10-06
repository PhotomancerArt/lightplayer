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
#   scripts/ota/emu-cut-sweep.sh <x-image> <y-image> <out> <cut>...
#
# <x-image>/<y-image> are `scripts/ota/build-image.sh` outputs. `Z=1` lets
# the board take encoding 1 (the default is raw only, `--ota-no-z`).
# `JOBS` runs that many cuts at once (default 4). Results: <out>/summary.tsv.
set -uo pipefail

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

one() {
    local n="$1" c="$out/cut-$1"
    cp "$x/merged.bin" "$c.flash"
    "$cli" emu run --rom-up-flash "$c.flash" --host-link --reboot-on-reset \
        --ota-offer "$y/ota" $z_flag --ota-cut-after "$n" --timeout 300s --wall-timeout 1200 \
        --console "$c.cut.txt" >/dev/null 2>"$c.cut.err"
    "$cli" emu run --rom-up-flash "$c.flash" --host-link --reboot-on-reset \
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
    final=$(grep -a "\[CORE\] core @" "$c.rec.txt" | tail -1 | grep -o "build [^ ]*\|engine [0-9]* B" | tr '\n' ' ')
    if grep -aq "done: UpToDate" "$c.rec.txt" && [[ "$final" == *"build $y_build"*"engine "* ]]; then
        verdict=PASS
    else
        verdict=FAIL
    fi
    printf '%s\tcut=%s\tserving=%s\tafter-cut=%s\trecovery-served=%s\tfinal=%s\n' \
        "$verdict" "$n" "${at:-none}" "$first" "${served:-0}" "$final"
}
export -f one
export x y out cli y_build z_flag
printf '%s\n' "$@" | xargs -P "${JOBS:-4}" -I{} bash -c 'one {}' | sort -t= -k2 -n | tee "$out/summary.tsv"
echo "pass: $(grep -c ^PASS "$out/summary.tsv") / $(wc -l < "$out/summary.tsv" | tr -d ' ')"
