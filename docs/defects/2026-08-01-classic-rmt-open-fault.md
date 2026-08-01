# Classic ESP32: opening a WS281x channel faults and reset-loops the board

- **Date:** 2026-08-01
- **Status:** OPEN — blocks M4-P3/P4 of the classic bring-up roadmap
- **Board:** DOM-Z-102 (classic ESP32 rev v3.1), `fw-esp32v3` @ `8836b4154`
- **Plan:** `2026-07-31-1444-classic-esp32-bringup`, M4-P2

## Symptom

With a project whose outputs bind real endpoints (`quad-strips-v3`,
`ws281x:rmt:IO18/IO16/IO14/IO2`), the board enters a reset loop: 19 boots in
a 25-second capture. Without a project — or with any project whose endpoints
do not resolve — it is completely stable (898 fps idle, single boot).

## What is proven

| Configuration | Result |
|---|---|
| No project loaded | **stable**; driver reports `4 of 4 usable RMT TX channels (blocks/channel=2 slot_stride=2 window_words=128 half_words=64)` |
| Project loaded, `open()` returns `Err` immediately | **stable**; project auto-loads, engine reports the refusal |
| Project loaded, `open()` runs but `clear_ram` + `configure_default_clock` removed | **still reset-loops** |
| Project loaded, full `open()` | reset-loops |

So the fault is **inside `Esp32V3RmtWs281xDriver::open`, before the RMT RAM
clear and the clock configuration** — i.e. in the endpoint/lease/slot
resolution, the `AnyPin::steal`, or `bind_channel`'s transmitter handoff.

Ruled out by measurement, not by reasoning:

- **Not stack exhaustion.** 110 KB heap (`.stack` 43,400 B) and 64 KB heap
  (`.stack` ≈ 93 KB) fault identically.
- **Not the RMT RAM base.** `RAM_BASE = 0x3FF5_6800` is byte-identical to the
  experiment repo's silicon-verified constant.
- **Not a double panic from interrupts.** Masking interrupts first changes
  nothing.

## The second defect: this fault cannot report itself

The panic channel is unusable for faults in this path. Every variant —
interrupt masking, draining the TX FIFO before printing, printing the line
number before the path, emitting the path in 4-byte chunks with drains —
produced the same ~5 characters (`at /U`, `L553`) before the chip reset, in
well under a millisecond. That is the signature of a **second exception taken
inside exception context** (window overflow, or a flash-mapped `.rodata` read
with `PS.EXCM` set), which vectors to reset and cannot be out-run from Rust.

`L553` is the one datum that escaped, and it is not attributable: the file
was truncated to `/U`, and several linked crates have a line 553.

**Consequence for the roadmap:** `lp-recovery`'s RTC-RAM ledger — currently
deferred to M7 as "not P1's scope" — is not a nicety. It is the only
instrument that can name a fault in the driver path on this chip, because it
stages a breadcrumb that survives the reset and reports on the next boot.
Whoever picks up this defect should land the ledger first; the alternative is
bisecting by deletion at ~4 minutes per build/flash/capture cycle.

## Reproduce

```bash
just build-fw-esp32v3   # add --features ws281x_telemetry for counters
espflash flash --chip esp32 --port /dev/cu.wchusbserial1140 \
  --partition-table lp-fw/fw-esp32v3/partitions.csv --flash-size 4mb \
  --baud 921600 --after hard-reset \
  target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3
cargo run -p lp-cli -- upload projects/test/quad-strips-v3 \
  serial:/dev/cu.wchusbserial1140
```

Then watch the port at 921600 and count `[INIT] fw-esp32v3 boot` lines. Note
`espflash monitor` alone stub-halts the app on this board; use
`flash --monitor --monitor-baud 921600` under a pty, or read the port
directly.

To clear a wedged auto-load: `espflash erase-region --port … 0x310000 0xF0000`.

## Not yet tried

- The experiment repo's `probe_ram_address` / `RamProbe` (`esp32_rmt.rs:537`,
  ~45 lines) — deliberately not ported; would assert the RMT RAM window on
  silicon at boot.
- Bisecting `open()` below `bind_channel` (pin steal vs. transmitter handoff).
- Comparing the esp-hal channel-creation path against `led-lab-esp32`'s,
  which runs the same backend on the same silicon without a registry/lease
  layer in front of it — the layer this port added is the prime suspect.
