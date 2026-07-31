#!/usr/bin/env bash
# M3 hardware walk — the ESP32-S3 app layer, end to end.
#
# Flashes the S3, then drives the walk that is M3's acceptance: the board boots,
# a host connects over serial, a project pushes and loads with every node kind
# gated off, and the crash path reports with real frames.
#
# Usage:  scripts/m3-hardware-walk.sh [port]
#
# With no port it identifies the S3 itself. That matters: several boards live on
# the desk bus, they renumber across replugs, and during M3 the S3 moved from
# usbmodem1101 to usbmodem1301 with a C6 taking the old name. espflash refuses a
# chip mismatch, so a wrong port fails safe — but confusingly.
#
# espflash hazard: `flash --monitor` streams forever, and SIGTERM/SIGKILL wedges
# the USB-Serial-JTAG port until someone physically replugs the board. This
# script always exits it with SIGINT.
set -euo pipefail

cd "$(dirname "$0")/.."
LOG="${TMPDIR:-/tmp}/m3-walk-$$.log"

port="${1:-}"
if [[ -z "$port" ]]; then
    echo "==> identifying the ESP32-S3"
    for p in /dev/cu.usbmodem*; do
        [[ -e "$p" ]] || continue
        if espflash board-info --port "$p" 2>/dev/null | grep -q "esp32s3"; then
            port="$p"
            echo "    found: $port"
            break
        fi
        echo "    skipping $p (not an S3, or busy)"
    done
fi
if [[ -z "$port" ]]; then
    echo "No ESP32-S3 found. Is it plugged in? Is another session holding it?" >&2
    exit 1
fi

if pgrep -f espflash >/dev/null 2>&1; then
    echo "WARNING: an espflash is already running; it may be holding the port." >&2
    pgrep -fl espflash >&2
fi

echo "==> flashing $port"
script -q "$LOG" just flash-fw-esp32s3 "$port" >/dev/null 2>&1 &
flash_pid=$!

for _ in $(seq 1 180); do
    grep -qa "starting server loop" "$LOG" 2>/dev/null && break
    kill -0 "$flash_pid" 2>/dev/null || break
    sleep 1
done

if ! grep -qa "starting server loop" "$LOG" 2>/dev/null; then
    echo "Board did not reach the server loop. Full log:" >&2
    sed 's/\x1b\[[0-9;]*m//g' "$LOG" | tail -40 >&2
    pkill -INT -f "espflash flash" 2>/dev/null || true
    exit 1
fi

echo
echo "===== BOOT ====="
sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -aE "INIT|RECOVERY|hardware manifest|proto=|boot:" | head -20

echo
echo "===== the walk ====="
echo "The monitor is still attached, streaming to: $LOG"
echo
echo "In another shell, drive the walk:"
echo
echo "  cargo run -p lp-cli -- upload examples/basic serial:$port"
echo "  cargo run -p lp-cli -- dev examples/basic serial:$port --push"
echo
echo "Then confirm in the log above:"
echo "  * the project LOADS (a gated-out node must not reject the project)"
echo "  * 'does not produce slot' appears — expected: with every gate off and"
echo "    NullGraphics nothing can produce pixels, so the readout stays silent"
echo "    by design. Its first real exercise is M4."
echo
echo "Press Ctrl-C here when done; the script exits espflash cleanly."

wait "$flash_pid" 2>/dev/null || true
pkill -INT -f "espflash flash" 2>/dev/null || true
sleep 2
pgrep -fl espflash || echo "port free"
