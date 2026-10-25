#!/usr/bin/env bash
# The ESP32-S3 machine's speed probe — an ORACLE, never a gate.
#
#   just bench-emu-esp32s3                          # build, run, table
#   scripts/emu/bench-esp32s3.sh --bin <dir> --no-build      # a saved build
#   scripts/emu/bench-esp32s3.sh --extra-flags "--no-block-cache"
#   scripts/emu/bench-esp32s3.sh --rows render-basic --runs 3 --json out.json
#
# `bench-esp32v3.sh`'s twin; read that header too. What is the S3's own:
#
# **The images are CI's**, pinned by `--images <dir>` (default
# `$LP_CI_IMAGES/esp32s3`, from `just fetch-ci-images`): the shipped image
# with Bluetooth ON (the default since #1254) as `merged.bin`. A row is only
# comparable with a row taken from the same bytes, so the table names the
# image's sha256.
#
# **Three rows, not interchangeable:**
#
#   boot-idle      the shipped image, ROM-up, no project, 1 s emulated, run by
#                  lp-emu-esp32s3. The radio's tasks wake it, but it sleeps in
#                  `waiti` most of the time and the machine skips that: the
#                  fastest row and the least interesting one.
#   render-basic   THE ROW TO QUOTE. The shipped image, ROM-up, Bluetooth on,
#                  `projects/test/basic` (241 lamps) uploaded over the hosted
#                  USB link (`lp-cli emu run --chip esp32s3 --host-link
#                  --upload`), rendering. The core is ~92 % busy here.
#   render-620     the same with `projects/test/skirt-620`, 620 lamps.
#
# A render row runs TWICE per sample, to 1 s and to 4 s emulated, and its
# steady-state figure is the difference: the first second is the boot and
# the upload (the D4 cache-off watch single-steps the boot's flash work), the
# next three are what a walk sits in. `rt` is emulated seconds per user
# second over those three — `0.25×` is 4 s of CPU per emulated second.
#
#   user s     USER CPU seconds, best of --runs. The number that survives a
#              busy desk; compare THESE across binaries, back to back.
#   wall s     wall clock of the same run. Inflated by anything else running;
#              the load average is printed beside the table.
#   instr      instructions retired (both cores; core 1 is held on the S3).
#   MIPS       instr / user s.
#   rt(user)   emulated s / user s (render rows: the steady state above).
#
# Identity is NOT this script's job: `scripts/emu/s3-oracle.sh` compares the
# bytes, counts and frames of two legs. Emulated microseconds never gate
# anything (AGENTS.md); this prints, it does not judge.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

bin="target/release"
images="${LP_CI_IMAGES:+$LP_CI_IMAGES/esp32s3}"
do_build=1
runs=2
rows="boot-idle render-basic render-620"
extra_flags=""
json_out=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bin) bin="${2:?}"; do_build=0; shift 2 ;;
        --no-build) do_build=0; shift ;;
        --images) images="${2:?}"; shift 2 ;;
        --runs) runs="${2:?}"; shift 2 ;;
        --rows) rows="${2:?}"; shift 2 ;;
        --extra-flags) extra_flags="${2?}"; shift 2 ;;
        --json) json_out="${2:?}"; shift 2 ;;
        -h|--help) sed -n '2,46p' "$0"; exit 0 ;;
        *) echo "bench-esp32s3: unknown option $1" >&2; exit 2 ;;
    esac
done
[[ -n "$images" && -f "$images/merged.bin" ]] || {
    echo "bench-esp32s3: no images: pass --images <dir> or set LP_CI_IMAGES (just fetch-ci-images)" >&2
    exit 2
}
if [[ $do_build -eq 1 ]]; then
    cargo build --release -p lp-cli -p lp-emu-esp32s3
fi
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# One timed run: prints "<user> <wall> <instructions>".
run_once() {
    local row="$1" window="$2" err="$scratch/err" out="$scratch/out"
    case "$row" in
        boot-idle)
            # shellcheck disable=SC2086
            /usr/bin/time -p "$bin/lp-emu-esp32s3" --boot-mode rom-up \
                --merged "$images/merged.bin" --timeout "$window" --usb-host attached \
                $extra_flags >"$out" 2>"$err"
            local instr
            instr="$(sed -nE 's/^run: .*instructions=([0-9]+).*/\1/p' "$out")"
            ;;
        render-basic|render-620)
            local project=projects/test/basic
            [[ "$row" == render-620 ]] && project=projects/test/skirt-620
            # shellcheck disable=SC2086
            /usr/bin/time -p "$bin/lp-cli" emu run --chip esp32s3 --host-link \
                --merged "$images/merged.bin" --upload "$project" --timeout "$window" \
                --wall-timeout 36000 --link-nonce 5a3e0001 $extra_flags >"$out" 2>"$err"
            local instr
            instr="$(sed -nE 's/^emu: ([0-9]+) instructions$/\1/p' "$err")"
            ;;
        *) echo "bench-esp32s3: unknown row $row" >&2; exit 2 ;;
    esac
    local user wall
    user="$(awk '/^user/ {print $2}' "$err")"
    wall="$(awk '/^real/ {print $2}' "$err")"
    echo "$user $wall ${instr:-0}"
}

# Best (lowest user s) of $runs: prints "<user> <wall> <instructions>".
best_of() {
    local row="$1" window="$2" best=""
    for _ in $(seq "$runs"); do
        local r
        r="$(run_once "$row" "$window")"
        if [[ -z "$best" ]] || awk -v a="${r%% *}" -v b="${best%% *}" 'BEGIN{exit !(a<b)}'; then
            best="$r"
        fi
    done
    echo "$best"
}

sha="$(shasum -a 256 "$images/merged.bin" | cut -c1-12)"
printf 'bench-esp32s3: %s, image merged.bin %s, flags "%s", best of %s\n' \
    "$bin" "$sha" "$extra_flags" "$runs"
printf '%-22s %9s %9s %14s %8s %9s\n' row "user s" "wall s" instr MIPS "rt(user)"
json_rows=()
for row in $rows; do
    if [[ "$row" == boot-idle ]]; then
        read -r user wall instr <<<"$(best_of "$row" 1s)"
        mips="$(awk -v i="$instr" -v u="$user" 'BEGIN{printf "%.1f", i/u/1e6}')"
        rt="$(awk -v u="$user" 'BEGIN{printf "%.3f", 1/u}')"
    else
        read -r u1 w1 i1 <<<"$(best_of "$row" 1s)"
        read -r u4 w4 i4 <<<"$(best_of "$row" 4s)"
        user="$(awk -v a="$u4" -v b="$u1" 'BEGIN{printf "%.2f", a-b}')"
        wall="$(awk -v a="$w4" -v b="$w1" 'BEGIN{printf "%.2f", a-b}')"
        instr=$((i4 - i1))
        mips="$(awk -v i="$instr" -v u="$user" 'BEGIN{printf "%.1f", i/u/1e6}')"
        rt="$(awk -v u="$user" 'BEGIN{printf "%.3f", 3/u}')"
    fi
    label="$row"
    [[ "$row" != boot-idle ]] && label="$row (1→4 s)"
    printf '%-22s %9s %9s %14s %8s %8s×\n' "$label" "$user" "$wall" "$instr" "$mips" "$rt"
    json_rows+=("{\"row\":\"$row\",\"user_s\":$user,\"wall_s\":$wall,\"instructions\":$instr,\"rt_user\":$rt}")
done
echo "load: $(uptime | sed -E 's/.*load averages?: //')"
if [[ -n "$json_out" ]]; then
    (IFS=,; echo "{\"bin\":\"$bin\",\"image_sha256_12\":\"$sha\",\"flags\":\"$extra_flags\",\"rows\":[${json_rows[*]}]}") >"$json_out"
fi
