#!/usr/bin/env bash
# Build the MERGED flash image a ROM-up boot needs: the IDF second-stage
# bootloader at 0x0, the partition table at 0x8000, and the app at 0x10000,
# in one file that is the whole chip — 4 MiB on the C6 and the classic, 8 MiB
# on the S3 (see the flash-size note below).
#
#   scripts/emu/build-merged-image.sh [--chip esp32c6|esp32|esp32s3] <app.elf> [<out.bin>]
#   → <out.bin>   (default: <app.elf dir>/merged.bin; sha256 written beside it)
#
# `--chip` is additive and defaults to `esp32c6`, which is what every caller
# written before M5 asks for by saying nothing. The classic ESP32 names itself
# (`--chip esp32`, espflash's own spelling — we record the revision as
# `esp32v3`, espflash does not know it) and takes `lp-fw/fw-esp32v3`'s
# partition table with it. `--chip esp32s3` (M6 P08) takes
# `lp-fw/fw-esp32s3`'s.
#
# ⚠️ **The flash size is a per-chip value, not a constant.** Both the C6 and
# the classic are 4 MB; the S3's partition table does not fit a 4 MB part and
# deliberately does not try to (`docs/adr/2026-07-30-esp32s3-partition-floor.md`
# — factory 6 MB at 0x010000, `lpfs` 1.5 MB at 0x610000), so its merged image
# is **8 MB**. A merged image built at the wrong size is not a smaller chip;
# it is a file whose partition table points past its own end.
#
# Direct load (M3/M4) puts the app's segments straight into memory and stages
# the flash-resident half at offsets the loader computes. A ROM-up boot reads
# the same three things a flasher wrote, in the layout `esptool`/`espflash`
# wrote them, and that layout is what this produces — the same command a desk
# flash runs, with `--merge` instead of a port.
#
# # Why espflash's own bundled bootloader, and why the version is pinned
#
# `espflash save-image --merge` embeds
# `resources/bootloaders/esp32c6-bootloader.bin` from the espflash crate. In
# espflash 3.3.0 that binary is
#
#     ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt 2nd stage bootloader
#     compile time Jun  7 2023 08:02:08
#
# which is the bootloader the silicon `boot-idle-flash` transcript logged,
# byte for byte — the same three `load:` lines and the same `entry`. A
# different espflash bundles a different bootloader and the boot-log diff
# would be comparing two programs, so the version is checked here rather
# than discovered in a failing gate.
#
# The flash mode and frequency come from the silicon transcript's own words
# (`mode:DIO, clock div:2`, `SPI Speed : 40MHz`), which are also espflash's
# defaults for this chip; they are spelled out so a default that moves is a
# visible change and not a silent one.
set -euo pipefail

chip=esp32c6
if [[ "${1:-}" == "--chip" ]]; then
    chip="${2:?--chip needs a value: esp32c6, esp32 or esp32s3}"
    shift 2
fi
# `esp32v3` is our name for the part; espflash does not know it.
[[ "$chip" == "esp32v3" ]] && chip=esp32

elf="${1:?usage: build-merged-image.sh [--chip esp32c6|esp32|esp32s3] <app.elf> [<out.bin>]}"
out="${2:-$(dirname "$elf")/merged.bin}"

repo="$(cd "$(dirname "$0")/../.." && pwd)"
# The partition table and the part's SIZE together: the two facts a merged
# image is wrong about in the same way if either comes from the wrong chip.
# `flash_size` mirrors `ChipSpec::flash_size` in
# `lp-emu/lp-emu-validate/src/driver.rs`, the runner's copy of the same table.
case "$chip" in
    esp32c6)
        partitions="$repo/lp-fw/fw-esp32c6/partitions.csv"
        flash_size=4mb
        ;;
    esp32)
        partitions="$repo/lp-fw/fw-esp32v3/partitions.csv"
        flash_size=4mb
        ;;
    esp32s3)
        partitions="$repo/lp-fw/fw-esp32s3/partitions.csv"
        # EIGHT megabytes — see the header. `lp-fw/builds/esp32s3-8mb.json`'s
        # `flashSizeMb` is the canonical source; this is one of its mirrors.
        flash_size=8mb
        ;;
    *)
        echo "build-merged-image: --chip $chip: known chips are esp32c6, esp32 (alias esp32v3) and esp32s3" >&2
        exit 2
        ;;
esac

# The espflash whose bundled bootloader the committed transcripts came from.
# A newer one is not automatically wrong — it is a different bootloader, and
# the gate that diffs a boot log has to be told so on purpose.
want_espflash="${LP_EMU_ESPFLASH_VERSION:-3.3.0}"

# `command -v` **before** running it, and the version read guarded against
# `set -e`. Both halves are the fix for a real CI failure: the first version
# of this check ran `espflash --version | awk …` inside a command
# substitution, and with `set -euo pipefail` a missing binary makes that
# pipeline exit 127 and takes the whole script with it — so the named error
# below, which exists precisely to say what is missing, was unreachable in
# the one case it was written for. `Emulator C6 (x64)` reported a bare
# `exit status: 127` and nothing else.
if ! command -v espflash >/dev/null 2>&1; then
    echo "build-merged-image: MISSING TOOL — \`espflash\` is not on PATH." >&2
    echo "  A merged image is the bootloader, the partition table and the app at" >&2
    echo "  their flash offsets, and the second-stage bootloader comes out of" >&2
    echo "  espflash's own bundled resources — there is nowhere else in this" >&2
    echo "  repository to get it." >&2
    echo "  Install it:  cargo binstall --no-confirm --locked espflash@$want_espflash" >&2
    echo "           or:  cargo install espflash --version $want_espflash --locked" >&2
    exit 127
fi

have_espflash="$(espflash --version 2>/dev/null | awk '{print $2}')" || have_espflash=""
if [[ "$have_espflash" != "$want_espflash" ]]; then
    echo "build-merged-image: WRONG TOOL VERSION — espflash is ${have_espflash:-unreadable}, the transcripts' bootloader ships with $want_espflash." >&2
    echo "  A different espflash bundles a different second-stage bootloader, so the boot log" >&2
    echo "  would differ for a reason that has nothing to do with the emulator." >&2
    echo "  Install it (\`cargo binstall --no-confirm --locked espflash@$want_espflash\`) or set" >&2
    echo "  LP_EMU_ESPFLASH_VERSION to say the change is intended." >&2
    exit 1
fi

if [[ ! -f "$elf" ]]; then
    echo "build-merged-image: $elf does not exist — build it first (scripts/emu/build-reference-image.sh)" >&2
    exit 1
fi

mkdir -p "$(dirname "$out")"
# Publish by `mv`, for the reason `build-reference-image.sh` spells out: a
# test process may be reading this path while another writes it.
staging="$out.partial"
espflash save-image \
    --chip "$chip" \
    --merge \
    --partition-table "$partitions" \
    --flash-size "$flash_size" \
    --flash-mode dio \
    --flash-freq 40mhz \
    "$elf" "$staging"

shasum -a 256 "$staging" | sed "s|$staging|$(basename "$out")|" > "$out.sha256"
echo "chip=$chip elf=$elf espflash=$have_espflash partitions=$partitions flash_size=$flash_size" > "$out.provenance"
mv "$staging" "$out"
echo "build-merged-image: done → $out"
cat "$out.sha256"
