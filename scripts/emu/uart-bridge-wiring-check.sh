#!/usr/bin/env bash
# uart-bridge-wiring-check.sh <bridge-MAC> <dut-MAC> <out-file> [seconds]
#
# Prove the three wires between two boards, with NO change to the board under
# test. The bridge is already flashed and running (`uart-bridge-flash.sh`); the
# DUT runs whatever it runs.
#
#   1. open the BRIDGE's USB port read-only, changing no line state
#      (scripts/emu/tty-capture.py — `stty` would assert DTR and reboot it);
#   2. reset the DUT from the DUT's OWN port with `espflash reset`, so the mask
#      ROM prints its banner on UART0 at 115,200 whatever the DUT's image does;
#   3. read what came through.
#
# A pass looks like `ESP-ROM:esp32c6-20220919` and `rst:0x… boot:0x…` appearing
# in the capture — the DUT's bytes, arriving over the wire, through the bridge,
# out of a port on a board that was never touched.
#
# This is the ONE step that deliberately holds two ports at once, which is why
# it does not go through desk-espflash-step.sh (that refuses if any usbmodem is
# held, correctly, for every other step). The discipline it keeps instead: the
# only writer is one foreground espflash on the DUT's own port, the reader
# opens the bridge and nothing else, and both are gone before it returns.
#
#   scripts/emu/uart-bridge-wiring-check.sh A0:F2:62:86:7E:44 A0:F2:62:87:49:A0 \
#       capture.txt 25
#
# `--no-stub` for the same reason `uart-bridge-flash.sh` uses it: espflash
# 3.3.0's stub times out connecting to both boards of this fixture.
#
# If nothing arrives, in this order: is the bridge running (does its port carry
# the two boot lines after a reset)? Are D6/D7 crossed rather than
# straight-through — on the XIAO ESP32-C6, D6 is GPIO16/U0TXD and D7 is
# GPIO17/U0RXD, and one board's TX must meet the other's RX? Is GND joined? Do
# NOT rewire to find out — that is a pair of hands, not an agent.
set -uo pipefail

bridge_mac="${1:?usage: uart-bridge-wiring-check.sh <bridge-MAC> <dut-MAC> <out> [seconds]}"
dut_mac="${2:?}"
out="${3:?}"
seconds="${4:-25}"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$here"

if [[ "${bridge_mac^^}" == "${dut_mac^^}" ]]; then
  echo "refusing: the bridge and the device under test are the same board" >&2
  exit 2
fi

bridge_port="$(python3 scripts/emu/board-port.py "$bridge_mac")" || exit 1
dut_port="$(python3 scripts/emu/board-port.py "$dut_mac")" || exit 1
echo "bridge $bridge_mac -> $bridge_port"
echo "DUT    $dut_mac -> $dut_port"

if pgrep -fl '^espflash' >/dev/null; then
  echo "PRECHECK: espflash is already running" >&2; pgrep -fl '^espflash' >&2; exit 9
fi
if lsof -n 2>/dev/null | grep -qE '/dev/(cu|tty)\.usbmodem'; then
  echo "PRECHECK: a usbmodem port is held" >&2; lsof -n | grep -E '/dev/(cu|tty)\.usbmodem' >&2; exit 9
fi

# The reader first, so the DUT's very first byte has somewhere to land.
python3 scripts/emu/tty-capture.py --dev "$bridge_port" --out "$out" \
    --seconds "$seconds" --baud 115200 &
reader=$!
sleep 2

echo "--- resetting the DUT from its own port ---"
script -q /dev/null /opt/homebrew/bin/python3 -c \
    'import signal,os,sys; signal.signal(signal.SIGINT, signal.SIG_DFL); os.execvp(sys.argv[1], sys.argv[1:])' \
    espflash reset --port "$dut_port" --chip esp32c6 --no-stub \
    --before default-reset --after hard-reset 2>&1 | tail -5

wait "$reader"

echo "--- what came through the bridge ---"
if [[ -s "$out" ]]; then
  head -c 2000 "$out"
  echo
  echo "--- ($(wc -c < "$out") bytes in $out) ---"
else
  echo "NOTHING. Read the failure ladder at the top of this script."
fi

echo "POSTCHECK lsof:"; lsof -n 2>/dev/null | grep -E '/dev/(cu|tty)\.usbmodem' || echo "  (ports free)"
echo "POSTCHECK pgrep:"; pgrep -fl '^espflash' || echo "  (no espflash)"
