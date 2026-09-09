#!/usr/bin/env bash
# The esp-emu RMT loopback recipe, as it actually is.
#
# `docs/reports/2026-09-08-esp-emu-rmt-loopback-differential.md` is the run
# this encodes. Two things the older recipe got wrong are fixed here:
#
#   * `--rmt-loopback` takes CHANNEL indices, not pins. `18:19` is refused
#     ("TX channel 18 out of range (0..2)"); the C6's RX channels start at 2,
#     so this payload's pair is `0:2`.
#   * The loopback splices a TX channel's symbol stream into an RX channel's
#     input. It is not a pad-to-pad wire, which is what our own `--wire 18:19`
#     is, so the two loopbacks are comparable at the words and not at the pin.
#
# esp-emu is NEVER in the repository and never in CI (DD19 authorises the
# bytes into a scratchpad, nothing more). Point $LP_ESP_EMU at the verified
# 0.42.0 binary — sha256 69df1ad1…671a, spike report §10 — the same variable
# `lp-emu-validate`'s driver reads.
#
# Known, and the reason this script cannot be a gate: the payload's records
# are written with `esp_println` (jtag-serial), and esp-emu's
# USB-Serial-JTAG model swallows every byte written to it. Only the
# harness's `log::info!` line, teed to UART0 by `spike_uart0_link`, comes
# out. Reading the received words needs the GDB stub, as the report does.
#
# Usage: scripts/emu/esp-emu-loopback.sh [TX:RX] [TIMEOUT]
set -euo pipefail

PAIR=${1:-0:2}
TIMEOUT=${2:-60s}
EMU=${LP_ESP_EMU:-esp-emu}
REPO=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
OUT=${LP_ESP_EMU_OUT:-${TMPDIR:-/tmp}/esp-emu-loopback}

mkdir -p "$OUT"

echo "==> building fw-esp32c6 (test_rmt_rx + spike_uart0_link)"
(
  cd "$REPO/lp-fw/fw-esp32c6"
  cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 \
    --features esp32c6,test_rmt_rx,spike_uart0_link
)

echo "==> merging a flashable image"
espflash save-image --chip esp32c6 --flash-size 4mb --merge \
  --partition-table "$REPO/lp-fw/fw-esp32c6/partitions.csv" \
  "$REPO/target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6" \
  "$OUT/rmt-rx.bin"

echo "==> esp-emu, RMT loopback $PAIR"
"$EMU" --chip esp32c6 --firmware "$OUT/rmt-rx.bin" \
  --rmt-loopback "$PAIR" \
  --timeout "$TIMEOUT" \
  --exit-on '[rmt-rx] === DONE ===' \
  --log-color never | tee "$OUT/esp-emu-$PAIR.txt"

echo
echo "==> ours, the same ELF, the pads tied in the signal fabric"
cargo run -q -p lp-emu-esp32c6 --release -- \
  --elf "$REPO/target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6" \
  --time-grade t1 \
  --usb-sj "file:$OUT/ours.cap" \
  --usb-host attached \
  --timeout 20s --wall-timeout 400 \
  --exit-on '[rmt-rx] === DONE ===' \
  --wire 18:19 \
  --trace RMT --trace-file "$OUT/ours-rmt-trace.txt"

echo
echo "our receiver's words are every 4-byte READ of RMT+0x580..0x63f in"
echo "$OUT/ours-rmt-trace.txt, in order: 1,537 per frame."
