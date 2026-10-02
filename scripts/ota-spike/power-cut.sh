#!/usr/bin/env bash
# OTA split-link spike, S3: cut the power at emulated time T (or, with
# CUT_BY_REQUESTS=1, right after the Nth update request is served) during an
# X → Y update, then boot what the flash holds and offer Y again. Pass = the
# board ends up running Y's engine; and right after the cut it is reachable
# (engine running, or core-only).
#
#   scripts/ota-spike/power-cut.sh <s3-dir> <cut-ms>...
#   (<s3-dir> holds x/ and y/ from build-split.sh; results in <s3-dir>/cuts/)
#
# Spike tooling — not product code.
set -uo pipefail

dir="$(cd "$1" && pwd)"
shift
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cli="$repo/target/release/lp-cli"
out="$dir/cuts"
mkdir -p "$out"

one() {
    local t="$1" c="$out/cut-$1"
    cp "$dir/x/merged.bin" "$c.flash"
    local cut_args=(--timeout "${t}ms")
    [[ -n "${CUT_BY_REQUESTS:-}" ]] && cut_args=(--ota-cut-after "$t" --timeout 120s)
    "$cli" emu run --rom-up-flash "$c.flash" --host-link --ota-offer "$dir/y" \
        --reboot-on-reset "${cut_args[@]}" --wall-timeout 900 \
        --console "$c.cut.txt" >/dev/null 2>"$c.cut.err"
    "$cli" emu run --rom-up-flash "$c.flash" --host-link --ota-offer "$dir/y" \
        --reboot-on-reset --timeout 70s --wall-timeout 1200 \
        --console "$c.rec.txt" >/dev/null 2>"$c.rec.err"
    # Where the cut landed: boots before it, the last OTA line, bytes served.
    local boots last served first final verdict
    boots=$(grep -ac "^\[CORE\] build" "$c.cut.txt")
    last=$(grep -a "\[OTA\]\|LpServer created\|CORE\] no engine" "$c.cut.txt" | tail -1 | cut -c1-60)
    served=$(grep -o "[0-9]* B served" "$c.cut.err" | cut -d' ' -f1)
    # Right after the cut: the first boot of the recovery run.
    first=$(awk '/^\[CORE\] build/{n++} n==1' "$c.rec.txt" |
        grep -a -m1 -o "LpServer created\|core-only mode" || echo "NOT REACHABLE")
    first="$(awk '/^\[CORE\] build/{print $3; exit}' "$c.rec.txt") $first"
    # The end: the last boot's build, and whether its engine ran.
    final=$(awk '/^\[CORE\] build/{b=$3; e=""} /LpServer created/{e="engine"} END{print b, e}' "$c.rec.txt")
    if [[ "$final" == *"+y engine" ]] && ! grep -aqi "panic" "$c.rec.txt"; then verdict=PASS; else verdict=FAIL; fi
    printf '%s\tcut=%sms\tboots-before=%s\tserved=%s\tlast="%s"\tafter-cut=%s\tfinal=%s\n' \
        "$verdict" "$t" "$boots" "${served:-0}" "$last" "$first" "$final"
}
export -f one
export dir out cli CUT_BY_REQUESTS
printf '%s\n' "$@" | xargs -P "${JOBS:-6}" -I{} bash -c 'one {}' | sort -t= -k2 -n | tee "$out/summary.tsv"
echo "pass: $(grep -c ^PASS "$out/summary.tsv") / $(wc -l < "$out/summary.tsv")"
