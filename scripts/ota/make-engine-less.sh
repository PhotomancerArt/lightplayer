#!/usr/bin/env bash
# Leave one C6 board without its engine: over USB, erase the engine
# header's sector of the board with this MAC, so its next boot is core-only
# ("no engine") and the next host that connects restores the engine (Y6,
# Y8). The same operation Part B's heal scenarios and the walk's
# `engine-less` step do to an emulated chip, done to a real one.
#
#   scripts/ota/make-engine-less.sh <MAC> [--dry-run]
#
# - The board is resolved by MAC, passively (scripts/emu/board-port.py):
#   never a port glob, never the first board on the bus. Any other board is
#   refused, and so is this one while the desk's lease tool (`board`) says
#   someone else holds it (set BOARD_HOLDER to the holder you took it as).
# - Where the engine is: the split image's engine starts either at the
#   region's start (a core at the high end) or on the first MMU page after a
#   core at the low end (lp-bootctl `SplitLayout::engine_room`). The script
#   reads both boot records (0x16000, 0x17000), takes the cores they name,
#   reads the engine header magic ("LPEH") at every place an engine can
#   start, and erases the ONE sector that holds it. No header: the board is
#   already engine-less, and nothing is written. Two: it refuses.
# - Nothing else is touched: the core, the boot records, the progress
#   record and the filesystem stay as they are. The chip is reset after.
#
# Needs espflash. Prints what it read and what it did; exit 0 on success
# (or already engine-less), non-zero with a reason otherwise.
set -euo pipefail

usage() { sed -n '2,25p' "$0" >&2; exit 2; }
[[ $# -ge 1 ]] || usage
mac="$1"
dry_run=0
[[ "${2:-}" == "--dry-run" ]] && dry_run=1
[[ "$mac" =~ ^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$ ]] || { echo "make-engine-less: not a MAC: $mac" >&2; exit 2; }

repo="$(cd "$(dirname "$0")/../.." && pwd)"
port="$(python3 "$repo/scripts/emu/board-port.py" "$mac")" || {
    echo "make-engine-less: no board with MAC $mac on the bus" >&2
    exit 1
}
if command -v board >/dev/null 2>&1 || [[ -x "$HOME/.local/bin/board" ]]; then
    board_tool="$(command -v board || echo "$HOME/.local/bin/board")"
    set +e
    "$board_tool" check "$mac" ${BOARD_HOLDER:+--as "$BOARD_HOLDER"} >/dev/null 2>&1
    held=$?
    set -e
    if [[ $held -eq 3 || $held -eq 4 ]]; then
        echo "make-engine-less: $mac is held by someone else (board check: $held); take it first" >&2
        exit 1
    fi
fi
echo "make-engine-less: board $mac on $port"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Both boot records, in one read. Each espflash call resets the chip into
# its bootloader and back (chaining calls with `--before no-reset` timed out
# uploading the stub on the fixture C6).
espflash read-flash --port "$port" 0x16000 0x2000 "$work/records.bin" >/dev/null

# The places an engine can start, from the cores the records name.
candidates="$(python3 - "$work/records.bin" <<'PY'
import struct, sys, zlib
REGION_START, PAGE = 0x18000, 0x8000
data = open(sys.argv[1], "rb").read()
spots = {REGION_START}
for i in range(2):
    rec = data[i * 0x1000 : i * 0x1000 + 28]
    magic, version, flags, seq, core_off, core_len, build, crc = struct.unpack("<IHHIIIII", rec)
    ok = magic == int.from_bytes(b"LPBR", "little") and zlib.crc32(rec[:24]) == crc
    print(f"record {i}: {'valid' if ok else 'none'}"
          + (f" seq {seq} core @{core_off:#x} +{core_len} {'trial' if flags & 1 else ''}" if ok else ""),
          file=sys.stderr)
    if ok and core_off == REGION_START:
        spots.add(-(-(core_off + core_len) // PAGE) * PAGE)
print(" ".join(f"{s:#x}" for s in sorted(spots)))
PY
)"

found=()
for at in $candidates; do
    espflash read-flash --port "$port" "$at" 0x1000 "$work/head.bin" >/dev/null
    magic="$(head -c 4 "$work/head.bin")"
    if [[ "$magic" == "LPEH" ]]; then
        echo "make-engine-less: engine header at $at"
        found+=("$at")
    else
        echo "make-engine-less: no engine header at $at"
    fi
done

case "${#found[@]}" in
    0)
        echo "make-engine-less: already engine-less; nothing written"
        espflash reset --port "$port" >/dev/null 2>&1 || true
        ;;
    1)
        if [[ $dry_run -eq 1 ]]; then
            echo "make-engine-less: --dry-run: would erase 0x1000 at ${found[0]}"
            espflash reset --port "$port" >/dev/null 2>&1 || true
        else
            espflash erase-region --port "$port" "${found[0]}" 0x1000 >/dev/null
            echo "make-engine-less: erased the engine header sector at ${found[0]} (0x1000 B); chip reset — it boots core-only"
        fi
        ;;
    *)
        echo "make-engine-less: engine headers at ${found[*]} — refusing to guess" >&2
        espflash reset --port "$port" >/dev/null 2>&1 || true
        exit 1
        ;;
esac
