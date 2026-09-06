---
status: open
found: 2026-09-06      # bench, L1 (the UART bridge harness), esp-emulator plan
fixed: not fixed — needs hands at the desk
area: bench fixture (two XIAO ESP32-C6s wired UART0-to-UART0), board A0:F2:62:86:7E:44
class: board-or-fixture
related: [lp2025/2026-09-06-1001-esp-emulator/g3-desk-batch.md, lp-fw/fw-esp32c6/src/tests/uart_bridge.rs]
---
# The bridge board never reaches its app: the second-stage bootloader dies before its first line

**Symptom** — XIAO ESP32-C6 `A0:F2:62:86:7E:44` (hub port 2,
`/dev/cu.usbmodem1433201`; board **B** of the UART-bridge fixture) boot-loops
about every 0.42 s. Every cycle is identical:

```text
ESP-ROM:esp32c6-20220919
Build:Sep 19 2022
rst:0x7 (TG0_WDT_HPSYS),boot:0x1e (SPI_FAST_FLASH_BOOT)
Saved PC:0x4086ed7a
SPIWP:0xee
mode:DIO, clock div:2
load:0x4086c410,len:0xd48
load:0x4086e610,len:0x2d68
load:0x40875720,len:0x1800
entry 0x4086c410
                        ← and then nothing, until the next reset
```

**Where it dies** — inside the **ESP-IDF second-stage bootloader**, before its
console is up. The mask ROM loads three bootloader segments and jumps to
`entry 0x4086c410`; a healthy board prints
`I (23) boot: ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt 2nd stage bootloader`
next (see
`lp-emu/transcripts/esp32c6/shader-compile-stress/silicon-seeed-xiao-esp32-c6-2026-09-06-d6cfaa205.txt`,
captured this morning on board `A0:F2:62:87:B4:8C`). This board prints nothing,
and the bootloader's own still-armed watchdog resets it.

`Saved PC:0x4086ed7a` falls inside the ROM's second bootloader segment
(`0x4086e610 .. 0x40871378`, from the `load:` line above), so it is a
bootloader address. **espflash's symbolizer calls it
`fw_esp32c6::board::esp32c6::init::init_board::HEAP` and that is a
coincidence, not a clue**: our app's `HEAP` static also begins at exactly
`0x4086e610` (it is `_stack_start` — the top of app RAM), so any address in
that 260 KB span resolves to it. The app is not running; it never started.

**Not the firmware.** The same boot loop, byte for byte, with the same
`Saved PC`, happens with `test_gpio_calibrate` — M2's payload, unmodified,
built from the same tree, the image that ran on `A0:F2:62:87:B4:8C` at
16:21 UTC today. Whatever this is, no change to a harness reaches it.

**Not the flash contents.** In the `test_gpio_calibrate` run espflash reported
`Segment at address '0x0' has not changed, skipping write` — the bootloader on
this board is byte-identical to the one espflash builds. Reflashing at
`--flash-freq 20mhz` (`mode:DIO, clock div:4` in the ROM line, so it took)
changes nothing.

## What was tried

| attempt | result |
|---|---|
| `test_uart_bridge`, DIO 40 MHz, `--no-stub` | writes and verifies; boot loop |
| `test_gpio_calibrate` (known-good control), same settings | writes; **identical** boot loop, identical `Saved PC` |
| `test_uart_bridge`, DIO **20 MHz** | ROM confirms `clock div:4`; boot loop |
| `espflash erase-flash` | refused: `espflash::stub_required`, and the stub cannot connect (below) |
| open the port read-only and watch | 3,108 bytes in 8 s — 19 boot cycles, nothing else |

## The other half: espflash's flash stub cannot connect to either board

`espflash 3.3.0` fails on **both** boards of this fixture, about five seconds
in, on any subcommand that uses the RAM stub:

```text
[INFO ] Connecting...
[INFO ] Using flash stub
Error: espflash::timeout
  × Error while connecting to device
  ╰─▶ Timeout while running command
```

With `--no-stub` the same command connects immediately and reads the chip
correctly (`esp32c6 (revision v0.2)`, 40 MHz crystal, 4MB,
`a0:f2:62:86:7e:44`). The same espflash used the stub successfully on
`A0:F2:62:87:B4:8C` at 16:21 UTC the same day, so this is not a tooling
regression on this Mac. Every script under `scripts/emu/` therefore passes
`--no-stub`, and `erase-flash` is simply unavailable until this is understood.

Two boards, two independent abnormalities, and the one thing they have that the
working board did not is **the three-wire fixture between them**. That is a
suspicion, not a finding: it was not tested, because testing it means
unplugging a wire, and an agent does not put hands on the bench.

## What would settle it (needs hands)

1. Unplug the three wires and power-cycle `A0:F2:62:86:7E:44` alone. If it
   boots, the fixture is the cause; if it still loops, the board is.
2. If it boots alone: check the wiring against the board's own pinout card.
   On the XIAO ESP32-C6, **D6 = GPIO16 = U0TXD** and **D7 = GPIO17 = U0RXD**,
   and the pair must **cross** — one board's D6 to the other's D7, both ways —
   with GND joined and **no** 3V3/5V link. Straight-through (D6–D6, D7–D7) ties
   two push-pull outputs together and leaves both receivers floating.
3. Re-run `scripts/emu/uart-bridge-flash.sh A0:F2:62:86:7E:44` and then
   `scripts/emu/uart-bridge-wiring-check.sh A0:F2:62:86:7E:44 A0:F2:62:87:49:A0
   capture.txt`. A pass is the device under test's `ESP-ROM:esp32c6-20220919`
   arriving in `capture.txt` — the DUT's bytes, out of a port on a board that
   was never flashed.

Board `A0:F2:62:87:49:A0` (board **A**) was not flashed and must not be: the
whole point of the fixture's verification is that the device under test is
untouched.
