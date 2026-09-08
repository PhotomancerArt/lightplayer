#!/usr/bin/env bash
# Build the MERGED flash image a ROM-up boot needs: the IDF second-stage
# bootloader at 0x0, the partition table at 0x8000, and the app at 0x10000,
# in one 4 MiB file that is the whole chip.
#
#   scripts/emu/build-merged-image.sh <app.elf> [<out.bin>]
#   → <out.bin>   (default: <app.elf dir>/merged.bin; sha256 written beside it)
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

elf="${1:?usage: build-merged-image.sh <app.elf> [<out.bin>]}"
out="${2:-$(dirname "$elf")/merged.bin}"

repo="$(cd "$(dirname "$0")/../.." && pwd)"
partitions="$repo/lp-fw/fw-esp32c6/partitions.csv"

# The espflash whose bundled bootloader the committed transcripts came from.
# A newer one is not automatically wrong — it is a different bootloader, and
# the gate that diffs a boot log has to be told so on purpose.
want_espflash="${LP_EMU_ESPFLASH_VERSION:-3.3.0}"
have_espflash="$(espflash --version 2>/dev/null | awk '{print $2}')"
if [[ "$have_espflash" != "$want_espflash" ]]; then
    echo "build-merged-image: espflash is $have_espflash, the transcripts' bootloader ships with $want_espflash." >&2
    echo "  A different espflash bundles a different second-stage bootloader, so the boot log" >&2
    echo "  would differ for a reason that has nothing to do with the emulator." >&2
    echo "  Install it (\`cargo install espflash --version $want_espflash\`) or set" >&2
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
    --chip esp32c6 \
    --merge \
    --partition-table "$partitions" \
    --flash-size 4mb \
    --flash-mode dio \
    --flash-freq 40mhz \
    "$elf" "$staging"

shasum -a 256 "$staging" | sed "s|$staging|$(basename "$out")|" > "$out.sha256"
echo "elf=$elf espflash=$have_espflash partitions=$partitions" > "$out.provenance"
mv "$staging" "$out"
echo "build-merged-image: done → $out"
cat "$out.sha256"
