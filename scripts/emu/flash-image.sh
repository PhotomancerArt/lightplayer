#!/usr/bin/env bash
# flash-image.sh <MAC> <sentinel> [elf] [max-secs]
#
# Flash one already-built ELF onto ONE named board and leave the port free.
# This is the desk discipline in one place; everything else in scripts/emu/
# that writes flash goes through it.
#
#   scripts/emu/flash-image.sh A0:F2:62:87:49:A0 "starting server loop"
#
# The board is named by MAC, never resolved for you. A bench with two boards of
# the same chip on it cannot answer "the C6", and `fwcheck port --chip esp32c6`
# cannot become an answer: it says a C6 is present, not which one. Getting it
# wrong writes over the board you meant to leave alone, silently.
#
# The wait is for <sentinel> — something the IMAGE prints — rather than for
# espflash's "Flashing has completed!", which only says bytes reached the
# flash. An image that writes fine and then boot-loops passes the second test
# and fails the first, and that is a distinction this bench has already needed.
#
# Everything else is inherited from desk-espflash-step.sh: foreground, under a
# pty, refuse outright if any usbmodem port is held rather than wait for it,
# SIGINT the espflash pid and never a pattern, confirm the port is free before
# returning.
#
# `--no-stub`: espflash 3.3.0's RAM stub times out connecting to the boards of
# this fixture (~5 s, `espflash::timeout`) while the same command without it
# connects immediately. See
# docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md.
set -euo pipefail

mac="${1:?usage: flash-image.sh <MAC> <sentinel> [elf] [max-secs]}"
sentinel="${2:?}"
elf="${3:-target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6}"
max="${4:-180}"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$here"

port="$(python3 scripts/emu/board-port.py "$mac")"
echo "board $mac -> $port"
case "$port" in
  /dev/cu.usbmodem*) ;;
  *) echo "refusing: $port is not a native-USB Espressif port" >&2; exit 2 ;;
esac
[[ -f "$elf" ]] || { echo "no such image: $elf" >&2; exit 2; }
echo "image: $elf ($(wc -c < "$elf") bytes)"

cap="${CAPTURE:-$(mktemp -t flash-image)}"
echo "capture: $cap"

status=0
PORT_DEV="$port" scripts/emu/desk-espflash-step.sh \
    "$cap" "$sentinel" "$max" -- \
    flash --chip esp32c6 --port "$port" --no-stub \
    --partition-table lp-fw/fw-esp32c6/partitions.csv \
    --flash-size 4mb --after hard-reset --monitor "$elf" || status=$?

if grep -qa 'TG0_WDT_HPSYS' "$cap" 2>/dev/null; then
    echo
    echo "⚠️  The board is boot-looping on TG0_WDT_HPSYS. Almost certainly the"
    echo "    wedged analog master, not this image: the second-stage bootloader"
    echo "    is spinning on LP_I2C_ANA_MAST_I2C0_BUSY, which lives in the LP"
    echo "    domain and survives every reset short of power-on."
    echo "    UNPLUG THE BOARD AND PLUG IT BACK IN, then re-run this."
    echo "    docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md"
fi
exit "$status"
