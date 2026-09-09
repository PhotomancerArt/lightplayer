#!/usr/bin/env bash
# uart-bridge-flash.sh <bridge-MAC> [fast]
#
# Put the `uart-bridge` payload on ONE named board and leave the port free.
#
# The board is named by MAC, never resolved for you: this exists for a bench
# with two identical C6s on it, where `fwcheck port --chip esp32c6` can say
# that a C6 is present but not which, and flashing the bridge onto the board
# under test destroys the measurement silently. Getting the argument wrong is
# the one mistake this script cannot catch — check it against
# `scripts/emu/board-port.py --list` before you run it.
#
#   scripts/emu/uart-bridge-flash.sh A0:F2:62:86:7E:44
#   scripts/emu/uart-bridge-flash.sh A0:F2:62:86:7E:44 fast   # 921,600
#
# Desk discipline, all of it inherited from desk-espflash-step.sh: one espflash
# at a time, in the foreground, under a pty; refuse outright if any usbmodem
# port is held rather than waiting for it; SIGINT the espflash pid, never a
# pattern; confirm the port is free before exiting. `--after hard-reset` so the
# board comes up running the bridge.
#
# Success looks like `SENTINEL after Ns` followed by `(port free)`.
set -euo pipefail

mac="${1:?usage: uart-bridge-flash.sh <bridge-MAC> [fast]}"
fast="${2:-}"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$here"

port="$(python3 scripts/emu/board-port.py "$mac")"
echo "bridge board $mac -> $port"

case "$port" in
  /dev/cu.usbmodem*) ;;
  *) echo "refusing: $port is not a native-USB Espressif port" >&2; exit 2 ;;
esac

features="test_uart_bridge,esp32c6"
[[ -n "$fast" ]] && features="$features,uart_bridge_fast"
# `touch src/main.rs` before a feature flip, as fw-esp32c6's own Cargo.toml
# says: this crate's build script watches its manifest directory, and a
# feature change alone has been seen to leave the previous ELF in place. The
# bridge at the wrong baud looks exactly like bad wiring, so the two seconds
# this costs are cheap.
touch lp-fw/fw-esp32c6/src/main.rs
( cd lp-fw/fw-esp32c6 && cargo build --features "$features" \
    --target riscv32imac-unknown-none-elf --profile release-esp32 )

# The desk discipline — and the `--no-stub` this fixture needs — lives in
# flash-image.sh. The sentinel is the payload's own readiness line rather than
# espflash's "Flashing has completed", so a pass means the image came up, not
# that bytes reached the flash.
cap="${CAPTURE:-$(mktemp -t uart-bridge-flash)}"
CAPTURE="$cap" scripts/emu/flash-image.sh "$mac" "UART-BRIDGE READY " \
    target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 180

echo "--- the two boot lines ---"
grep -a -E 'fw-checks-header|UART-BRIDGE READY' "$cap" || echo "(none — the bridge did not announce itself)"
