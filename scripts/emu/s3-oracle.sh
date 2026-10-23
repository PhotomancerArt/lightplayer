#!/usr/bin/env bash
# The ESP32-S3 machine's IDENTITY ORACLE: two legs of the same run, compared
# cell for cell. `scripts/emu/v3-oracle.sh`'s S3 twin.
#
#   scripts/emu/s3-oracle.sh <out-dir> <row> <window>
#   scripts/emu/s3-oracle.sh --bin-a <dir> --bin-b <dir> <out-dir> <row> <window>
#   scripts/emu/s3-oracle.sh --flags-a "" --flags-b "--no-block-cache" <out-dir> <row> <window>
#
# A leg is a directory holding `lp-cli` and `lp-emu-esp32s3` (default for
# both: `target/release`) and a flag string appended to every invocation.
# With no `--flags-*`, leg A runs as it is and leg B adds `--no-block-cache`:
# the block cache against the interpreter, in one binary — the free oracle.
# `--bin-a`/`--bin-b` with no flags runs two BINARIES as they are, which is
# the before/after leg of a speed change.
#
# # The rows
#
#   boot-idle      the shipped image (Bluetooth on, the default), ROM-up out of
#                  its merged chip, no project. lp-emu-esp32s3 directly.
#   radio-boot     the radio probe image (`radio_boot_probe`), ROM-up.
#   render-basic   the shipped image, ROM-up, `projects/test/basic` uploaded
#                  over the hosted link (`lp-cli emu run --chip esp32s3
#                  --host-link --upload`): 241 lamps, THE ROW TO QUOTE.
#   render-620     the same with `projects/test/skirt-620`.
#
# ⚠️ The render rows state the link's nonce (`--link-nonce`). A product host
# draws a fresh one per open, and so does `lp-cli emu run` by default — two
# hosted runs of one binary then differ by ~1,000 instructions and every
# frame's timestamp, which reads exactly like a fast path moving the guest.
#
# The images come from `--images <dir>` (default `$LP_CI_IMAGES/esp32s3`):
# the `ci-images-esp32s3` layout, `merged.bin` and `merged-radio.bin`
# (`just fetch-ci-images`). Pin them: a row is only comparable with a row
# taken from the same bytes.
#
# # The columns
#
#   usb      the USB-Serial-JTAG bytes the host read (lp-emu-esp32s3 rows), or
#            the hosted link's console transcript (render rows): the product's
#            own words.
#   uart     UART0's bytes: the mask ROM's and the bootloader's console.
#   run      the `run:` line (cycles, per-core instructions, idle skips) or the
#            hosted run's `N instructions` and host-link lines. The counts a
#            fast path could move and must not.
#   frames   every decoded WS281x frame, timestamps and bytes.
#
# Every column must read `same`. DIFFERENT is a stop, not a tolerance: a fast
# path that moves a cycle or a byte is a bug (AGENTS.md, "Never change what the
# guest observes"). `user s` is printed beside each leg for convenience and is
# never compared — `bench-esp32s3.sh` is the speed probe.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"

bin_a="$repo/target/release"
bin_b="$repo/target/release"
flags_a=""
flags_b="--no-block-cache"
have_flags=0
images="${LP_CI_IMAGES:+$LP_CI_IMAGES/esp32s3}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bin-a) bin_a="$(cd "${2:?}" && pwd)"; shift 2 ;;
        --bin-b) bin_b="$(cd "${2:?}" && pwd)"; shift 2 ;;
        --flags-a) flags_a="${2?}"; have_flags=1; shift 2 ;;
        --flags-b) flags_b="${2?}"; have_flags=1; shift 2 ;;
        --images) images="$(cd "${2:?}" && pwd)"; shift 2 ;;
        -h|--help) sed -n '2,45p' "$0"; exit 0 ;;
        -*) echo "s3-oracle: unknown option $1" >&2; exit 2 ;;
        *) break ;;
    esac
done
if [[ "$bin_a" != "$bin_b" && $have_flags -eq 0 ]]; then
    flags_b=""
fi
[[ $# -eq 3 ]] || { echo "usage: s3-oracle.sh [options] <out-dir> <row> <window>" >&2; exit 2; }
out="$1"; row="$2"; window="$3"
[[ -n "$images" && -f "$images/merged.bin" ]] || {
    echo "s3-oracle: no images: pass --images <dir> or set LP_CI_IMAGES (just fetch-ci-images)" >&2
    exit 2
}
mkdir -p "$out"

# One leg: <name> <bin-dir> <flags>. Writes $out/<row>.<name>.{usb,uart,run,frames,time}.
leg() {
    local name="$1" dir="$2" flags="$3" base="$out/$row.$1"
    rm -f "$base".*
    local start end
    case "$row" in
        boot-idle|radio-boot)
            local chip="$images/merged.bin"
            [[ "$row" == radio-boot ]] && chip="$images/merged-radio.bin"
            # shellcheck disable=SC2086
            /usr/bin/time -p "$dir/lp-emu-esp32s3" --boot-mode rom-up --merged "$chip" \
                --timeout "$window" --usb-host attached \
                --usb-sj "file:$base.usb" --uart0 "file:$base.uart" \
                --dump-frames "file:$base.frames" $flags \
                >"$base.stdout" 2>"$base.stderr" || true
            grep -E '^run:' "$base.stdout" >"$base.run" || true
            ;;
        render-basic|render-620)
            local project="$repo/projects/test/basic"
            [[ "$row" == render-620 ]] && project="$repo/projects/test/skirt-620"
            # shellcheck disable=SC2086
            /usr/bin/time -p "$dir/lp-cli" emu run --chip esp32s3 --host-link \
                --merged "$images/merged.bin" --upload "$project" \
                --timeout "$window" --wall-timeout 36000 --link-nonce 5a3e0001 \
                --console "$base.usb" --dump-frames "$base.frames" $flags \
                >"$base.stdout" 2>"$base.stderr" || true
            : >"$base.uart"
            grep -E '^emu: ([0-9]+ instructions|reached|host link)' "$base.stderr" >"$base.run" || true
            ;;
        *) echo "s3-oracle: unknown row $row" >&2; exit 2 ;;
    esac
    touch "$base.frames"
    grep -E '^user' "$base.stderr" | awk '{print $2}' >"$base.time" || true
}

leg a "$bin_a" "$flags_a"
leg b "$bin_b" "$flags_b"

cell() {
    if cmp -s "$out/$row.a.$1" "$out/$row.b.$1"; then echo same; else echo DIFFERENT; fi
}
status=0
printf '%-14s %-8s %-8s %-8s %-8s %10s %10s\n' row usb uart run frames "a user s" "b user s"
cols=()
for c in usb uart run frames; do
    v="$(cell "$c")"
    cols+=("$v")
    [[ "$v" == same ]] || status=1
done
printf '%-14s %-8s %-8s %-8s %-8s %10s %10s\n' "$row" "${cols[@]}" \
    "$(cat "$out/$row.a.time")" "$(cat "$out/$row.b.time")"
echo "  a: $bin_a ${flags_a:-(no flags)}"
echo "  b: $bin_b ${flags_b:-(no flags)}"
echo "  run: $(tr '\n' ' ' <"$out/$row.a.run")"
echo "  frames: $(wc -l <"$out/$row.a.frames" | tr -d ' ') (a), $(wc -l <"$out/$row.b.frames" | tr -d ' ') (b)"
if [[ $status -ne 0 ]]; then
    echo "s3-oracle: DIFFERENT — artefacts in $out/$row.{a,b}.*" >&2
fi
exit $status
