#!/usr/bin/env bash
# c6-bootloader-hang-walk.sh <board-MAC> [manifest.json]
#
# The regression walk for the first-flash bootloader hang on a fresh
# ESP32-C6 (docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md):
#
#   1. induce the factory state — LPPERI_CLK_EN bit 29 (LP analog I2C clock)
#      gated — over the ROM downloader, and PROVE it: the next boot must show
#      a `Saved PC` inside the bootloader's code segments;
#   2. flash through OUR host provider (lpa-link's espflash path, the same
#      code Studio's native flasher runs), which restores the clock before
#      its closing reset;
#   3. pass = the board answers with its hello (the smoke reaches Ready) AND
#      the flash log carries the "restored the LP analog I2C clock" line.
#
#   scripts/c6-bootloader-hang-walk.sh A0:F2:62:85:A8:7C
#   scripts/c6-bootloader-hang-walk.sh A0:F2:62:85:A8:7C target/studio-web-assets/firmware/esp32c6-4mb/manifest.json
#
# DESTRUCTIVE: the host-provider smoke ERASES the board and reflashes the
# packaged firmware (`just studio-firmware-package-esp32c6` builds it).
#
# Desk discipline (from scripts/emu/uart-bridge-flash.sh): the board is named
# by MAC and resolved passively; refuse if ANY usbmodem port is held (Studio
# holds its port exclusively — disconnect there first); one flasher at a
# time, in the foreground; confirm the port is free before exiting.
set -euo pipefail

mac="${1:?usage: c6-bootloader-hang-walk.sh <board-MAC> [manifest.json]}"
manifest="${2:-target/studio-web-assets/firmware/esp32c6-4mb/manifest.json}"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here"

fail() { echo "FAIL: $*" >&2; exit 1; }

# esptool's own interpreter carries the esptool + pyserial packages.
if [[ -z "${ESPTOOL_PYTHON:-}" ]]; then
  esptool_bin="$(command -v esptool || true)"
  [[ -n "$esptool_bin" ]] || fail "esptool not on PATH (brew install esptool) and ESPTOOL_PYTHON unset"
  esptool_bin="$(readlink -f "$esptool_bin")"
  ESPTOOL_PYTHON="$(dirname "$esptool_bin")/python"
  [[ -x "$ESPTOOL_PYTHON" ]] || ESPTOOL_PYTHON="$(head -1 "$esptool_bin" | sed 's/^#!//')"
fi
"$ESPTOOL_PYTHON" -c 'import esptool, serial' 2>/dev/null || fail "$ESPTOOL_PYTHON cannot import esptool/pyserial; set ESPTOOL_PYTHON"

[[ -f "$manifest" ]] || fail "no packaged manifest at $manifest — run: just studio-firmware-package-esp32c6"

if lsof /dev/cu.usbmodem* >/dev/null 2>&1; then
  lsof /dev/cu.usbmodem* >&2 || true
  fail "a usbmodem port is held — disconnect Studio (or whatever holds it) and rerun"
fi

port="$(scripts/emu/board-port.py "$mac")" || fail "no native-USB board with MAC $mac on the bus"
echo "== board $mac on $port"

tool="$ESPTOOL_PYTHON scripts/c6-lp-ana-i2c.py"

echo "== 1. induce: gate the LP analog I2C clock over the ROM"
$tool induce "$port"
sleep 1.5
echo "== 1b. prove the hang: Studio's normal reset, then watch"
if $tool watch "$port" --seconds 5 > /tmp/c6-walk-watch.txt 2>&1; then
  cat /tmp/c6-walk-watch.txt
  fail "the board booted its app with the clock gated — the fault was NOT induced (bootloader already immune? see P4)"
fi
grep -q "HUNG BOOTLOADER" /tmp/c6-walk-watch.txt || { cat /tmp/c6-walk-watch.txt; fail "no hung-bootloader signature after the reset"; }
grep "HUNG BOOTLOADER" /tmp/c6-walk-watch.txt
sleep 1

echo "== 2. flash through the host provider (erase + flash + wait for Ready)"
log=/tmp/c6-walk-smoke.txt
set +e
cargo run -q -p lpa-link --features host-serial-esp32 --example manage_smoke -- "$port" "$manifest" 2>&1 | tee "$log"
smoke_status=${PIPESTATUS[0]}
set -e
[[ $smoke_status -eq 0 ]] || fail "manage_smoke exited $smoke_status (see $log)"

echo "== 3. verdict"
grep -q "restored the LP analog I2C clock" "$log" || fail "the flash log has no 'restored the LP analog I2C clock' line (see $log)"
grep -q "state: Ready" "$log" || fail "the smoke never reached Ready (see $log)"

if lsof "$port" >/dev/null 2>&1; then
  lsof "$port" >&2 || true
  fail "$port is still held after the walk"
fi
echo "PASS: induced hang, flashed through the host provider, clock restored, board answered (port free)"
