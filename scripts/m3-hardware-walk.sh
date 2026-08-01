#!/usr/bin/env bash
# M3 hardware walk — the ESP32-S3 app layer, end to end.
#
# Flashes the S3, then drives M3's acceptance: the board boots, a host connects
# over serial, and a project pushes and loads with every node kind gated off.
#
# Usage:  scripts/m3-hardware-walk.sh [port]
#
# With no port it identifies the S3 itself. That matters: several boards live on
# the desk bus, they renumber across replugs, and during M3 the S3 moved from
# usbmodem1101 to usbmodem1301 with a C6 taking the old name. espflash refuses a
# chip mismatch, so a wrong port fails safe — but confusingly.
#
# Two hazards this script exists to handle, both of which cost real time when
# hit by hand:
#
#   * USB-Serial-JTAG is an EXCLUSIVE port. espflash's `--monitor` holds it, so
#     lp-cli cannot open it at the same time — you get "Device or resource
#     busy". The monitor is therefore detached before the lp-cli phase, not run
#     alongside it.
#   * SIGTERM/SIGKILL on espflash wedges the port until someone physically
#     replugs the board. This script always exits it with SIGINT.
set -euo pipefail

cd "$(dirname "$0")/.."
LOG="${TMPDIR:-/tmp}/m3-walk-$$.log"
PROJECT="${PROJECT:-examples/basic}"

port="${1:-}"
if [[ -z "$port" ]]; then
    echo "==> identifying the ESP32-S3"
    # Probes with per-port timeouts (bare `espflash board-info` can hang on a
    # wedged port); busy ports are skipped, not reset under their owner.
    port="$(cargo run -q -p lp-cli -- fwcheck port --chip esp32s3)"
    echo "    found: $port"
fi
if [[ -z "$port" ]]; then
    echo "No ESP32-S3 found. Is it plugged in? Is another session holding it?" >&2
    exit 1
fi

release_port() {
    pkill -INT -f "espflash flash" 2>/dev/null || true
    for _ in $(seq 1 15); do
        pgrep -f "espflash flash" >/dev/null 2>&1 || return 0
        sleep 1
    done
    echo "WARNING: espflash still holding the port." >&2
}
trap release_port EXIT

if pgrep -f espflash >/dev/null 2>&1; then
    echo "WARNING: an espflash is already running; it may be holding the port." >&2
    pgrep -fl espflash >&2
fi

# ---------------------------------------------------------------- flash + boot
echo "==> flashing $port"
script -q "$LOG" just flash-fw-esp32s3 "$port" >/dev/null 2>&1 &
flash_pid=$!

for _ in $(seq 1 180); do
    grep -qa "starting server loop" "$LOG" 2>/dev/null && break
    kill -0 "$flash_pid" 2>/dev/null || break
    sleep 1
done

if ! grep -qa "starting server loop" "$LOG" 2>/dev/null; then
    echo "Board did not reach the server loop. Tail of the log:" >&2
    sed 's/\x1b\[[0-9;]*m//g' "$LOG" | tail -40 >&2
    exit 1
fi

echo
echo "===== BOOT ====="
sed 's/\x1b\[[0-9;]*m//g' "$LOG" \
    | grep -aE "INIT|RECOVERY|hardware manifest|proto=|Boot:" | head -20

# The monitor owns the port exclusively; lp-cli cannot open it until it lets go.
echo
echo "==> detaching the monitor so lp-cli can open the port"
release_port
trap - EXIT

# ------------------------------------------------------------------- the walk
echo
echo "===== UPLOAD ====="
if cargo run -q -p lp-cli -- upload "$PROJECT" "serial:$port"; then
    echo "upload: OK"
else
    echo "upload: FAILED" >&2
    exit 1
fi

echo
echo "===== RELOAD (proves it persisted to flash) ====="
if cargo run -q -p lp-cli -- upload "$PROJECT" "serial:$port"; then
    echo "second upload: OK"
else
    echo "second upload: FAILED" >&2
    exit 1
fi

echo
echo "===== WALK PASSED ====="
cat <<'NOTE'
The board booted, served, and accepted a project over serial with every node
kind gated off and NullGraphics as the backend.

Expected and CORRECT, not a failure: the device logs
`does not produce slot "output"` each frame. With all eight gates off and a null
graphics backend nothing can produce pixels, so the readout stays silent by
design — its first real exercise is M4, when the shader node is switched on.

To watch the device's own log, reattach the monitor (it takes the port back, so
stop lp-cli first):

    just flash-fw-esp32s3 <port>

To iterate on a project instead of a one-shot upload:

    cargo run -p lp-cli -- dev <dir> --push serial:<port>
NOTE
