#!/usr/bin/env bash
# Vendor the Espressif mask-ROM ELFs the machine emulators load.
#
# Vision D6, plan PD7: **the ROM is loaded in every configuration**, because
# the application calls into the mask ROM at runtime whatever booted it —
# `rtc_get_reset_reason` from `__pre_init`, `ets_delay_us` from every clock
# path, `uart_tx_one_char` from esp-println, `esp_rom_spiflash_*` from
# esp-storage. So the ROM image is not an optional extra for a ROM-up boot;
# it is part of the memory map, and it is vendored rather than fetched at
# test time — a test that reaches GitHub fails offline and in CI sandboxes.
#
# The images come from Espressif's `esp-rom-elfs` release, which is
# **Apache-2.0** (the repository's LICENSE, vendored beside them and under
# `licenses/`). They are *binaries Espressif publishes*, not source we
# derived anything from, and they are committed verbatim with their
# checksums.
#
# The release's own `esp-rom-elfs-<tag>-checksum.sha256` is NOT what this
# script trusts: a checksum fetched from the same server as the artifact
# proves only that the download completed. TARBALL_SHA256 below is pinned in
# the repository, and was verified against the published checksum file once,
# by hand, when this script was written (2026-09-06).
#
# Usage:
#   scripts/emu/fetch-rom-elfs.sh            fetch, verify, vendor, write SHA256SUMS
#   scripts/emu/fetch-rom-elfs.sh --check    verify the vendored files only (no network)
#
# `--check` is what a machine with no network can run; the same assertion is
# a unit test in `lp-emu-esp32c6`, so a corrupted or swapped ROM fails the
# build's tests rather than a boot at cycle 400,000.
set -euo pipefail
cd "$(dirname "$0")/../.."

RELEASE_TAG="20260528"
TARBALL="esp-rom-elfs-${RELEASE_TAG}.tar.gz"
TARBALL_URL="https://github.com/espressif/esp-rom-elfs/releases/download/${RELEASE_TAG}/${TARBALL}"
TARBALL_SHA256="caa463d3cbef2430a5a35847c1d9f2f152403b17a802050927ff60c8da54fe46"
TARBALL_BYTES="4902355"
LICENSE_URL="https://raw.githubusercontent.com/espressif/esp-rom-elfs/${RELEASE_TAG}/LICENSE"

DEST="lp-emu/esp/roms"

# Which chips we vendor. One line per file, so adding the classic and the S3
# with plan three is one line each and the checksum file regenerates.
#
# Only the C6 is here today: plan one is the C6, and 2 MB of ROM images for
# chips no code loads yet is 2 MB of repository nobody can check.
WANTED=(
    "esp32c6_rev0_rom.elf"
)

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        sha256sum "$1" | awk '{print $1}'
    fi
}

check_only() {
    local fail=0
    if [[ ! -f "$DEST/SHA256SUMS" ]]; then
        echo "fetch-rom-elfs: $DEST/SHA256SUMS is missing" >&2
        exit 1
    fi
    while read -r want name; do
        [[ -z "$name" ]] && continue
        # The tarball itself is recorded for provenance but not vendored.
        if [[ "$name" == "$TARBALL" ]]; then
            if [[ "$want" != "$TARBALL_SHA256" ]]; then
                echo "MISMATCH: SHA256SUMS records a different tarball than this script pins"
                fail=1
            fi
            continue
        fi
        if [[ ! -f "$DEST/$name" ]]; then
            echo "MISSING: $DEST/$name"
            fail=1
            continue
        fi
        got="$(sha256 "$DEST/$name")"
        if [[ "$got" != "$want" ]]; then
            echo "MISMATCH: $DEST/$name"
            echo "    recorded $want"
            echo "    actual   $got"
            fail=1
        fi
    done <"$DEST/SHA256SUMS"
    if [[ "$fail" != 0 ]]; then
        echo
        echo "A vendored ROM does not match SHA256SUMS. Do not 'fix' the sums file:"
        echo "re-run scripts/emu/fetch-rom-elfs.sh, which re-derives both from the"
        echo "published tarball."
        exit 1
    fi
    echo "fetch-rom-elfs: vendored ROM ELFs match $DEST/SHA256SUMS"
}

if [[ "${1:-}" == "--check" ]]; then
    check_only
    exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "fetching $TARBALL_URL"
curl -fsSL -o "$work/$TARBALL" "$TARBALL_URL"

actual_bytes="$(wc -c <"$work/$TARBALL" | tr -d ' ')"
if [[ "$actual_bytes" != "$TARBALL_BYTES" ]]; then
    echo "fetch-rom-elfs: $TARBALL is $actual_bytes bytes, expected $TARBALL_BYTES" >&2
    exit 1
fi

actual_sha="$(sha256 "$work/$TARBALL")"
if [[ "$actual_sha" != "$TARBALL_SHA256" ]]; then
    echo "fetch-rom-elfs: checksum mismatch on $TARBALL" >&2
    echo "    expected $TARBALL_SHA256" >&2
    echo "    actual   $actual_sha" >&2
    exit 1
fi
echo "  sha256 OK ($actual_bytes bytes)"

tar xzf "$work/$TARBALL" -C "$work" "${WANTED[@]}"

mkdir -p "$DEST"
for name in "${WANTED[@]}"; do
    cp "$work/$name" "$DEST/$name"
    chmod 0644 "$DEST/$name"
    echo "  vendored $DEST/$name ($(wc -c <"$DEST/$name" | tr -d ' ') bytes)"
done

echo "fetching $LICENSE_URL"
curl -fsSL -o "$DEST/LICENSE" "$LICENSE_URL"
# The provenance rule wants the upstream licence text under `licenses/` too.
cp "$DEST/LICENSE" "licenses/Apache-2.0.txt"

{
    echo "$TARBALL_SHA256  $TARBALL"
    for name in "${WANTED[@]}"; do
        echo "$(sha256 "$DEST/$name")  $name"
    done
} >"$DEST/SHA256SUMS"

echo
check_only
