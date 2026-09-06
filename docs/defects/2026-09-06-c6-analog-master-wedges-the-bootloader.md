---
status: root-caused — needs one power cycle to confirm the cure
found: 2026-09-06      # bench, L1 (the UART bridge harness), esp-emulator plan
fixed: not fixed — the cure is a POWER CYCLE, which needs hands
area: bench fixture (two XIAO ESP32-C6s), ESP-IDF second-stage bootloader, LP_I2C_ANA_MST
class: sticky-analog-domain-state
related: [lp2025/2026-09-06-1001-esp-emulator/g3-desk-batch.md, scripts/emu/reset-and-capture.py]
---
# A wedged analog-master I²C stops the second-stage bootloader before its first line, and only power-on clears it

**Symptom** — a XIAO ESP32-C6 boot-loops about every 0.4 s. Every cycle is
identical, and the app never runs:

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

A healthy board prints `I (23) boot: ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt
2nd stage bootloader` immediately after `entry`.

## Root cause: the bootloader is spinning on `LP_I2C_ANA_MAST_I2C0_BUSY`

`Saved PC:0x4086ed7a` is a **bootloader** address, not an app one. The ROM's own
`load:` lines say the second bootloader segment lives at
`0x4086e610 .. 0x40871378`, and espflash's bundled
`resources/bootloaders/esp32c6-bootloader.bin` matches those three segments and
that entry point exactly. Offset `0x76a` into that segment disassembles to:

```asm
756: lui  a5, 0x600b2      ; a5 = 0x600B_2000
75a: sw   a6, 0x400(a5)    ; write the command word to LP_I2C_ANA_MST.I2C0_CTRL
75e: lui  a4, 0x600b2
762: lui  a3, 0x2000       ; a3 = 0x0200_0000  (lui shifts by 12) = BIT(25)
766: addi a0, a4, 0x400    ; a0 = 0x600B_2400
76a: lw   a5, 0x0(a0)      ; <-- Saved PC: re-read I2C0_CTRL
76c: and  a5, a5, a3
76e: bnez a5, 0x766        ; spin while BUSY is set
```

`0x600B_2400` is **`LP_I2C_ANA_MST.I2C0_CTRL`** (esp32c6 PAC 0.23.2,
`lp_i2c_ana_mst`), and bit 25 of it is **`LP_I2C_ANA_MAST_I2C0_BUSY`**. The
command word is `a6 = (a2 << 8) | a0`, the REGI2C block/register encoding, and
the surrounding code builds an msb/lsb field mask from two more arguments — this
is `regi2c_ctrl_write_reg_mask`, the analog-master read-modify-write.

So: **the bootloader asks the analog master to program an on-chip analog block
and waits forever for a transaction that never completes.** ESP-IDF does this
inside `bootloader_clock_configure()` (BBPLL bring-up), which runs *before*
`bootloader_console_init()` — hence no output, a fixed `Saved PC`, and the
watchdog as the only thing that moves.

Espressif's symbolizer labels `0x4086ed7a` as
`fw_esp32c6::board::esp32c6::init::init_board::HEAP`. **That is an address
coincidence, not a clue**: our app's `HEAP` static begins at exactly
`0x4086e610` (it is also `_stack_start`), so every address in that 260 KB span
resolves to it. The app is not running.

## Why nothing short of a power cycle fixes it

`LP_I2C_ANA_MST` is in the **LP (low-power) domain** — the `0x600B_xxxx` block.
`TG0_WDT_HPSYS` resets only the HP system, so the wedged transaction survives
its own reset loop. Measured on board `A0:F2:62:87:49:A0`, 2026-09-06, each of
these left the loop running unchanged:

| tried | result |
|---|---|
| espflash `--after hard-reset` at the end of a flash | loop continues |
| RTS pulse on the board's own USB-SJ handle (`reset-and-capture.py`) | `rst:0x15 (USB_UART_HPSYS)`, then straight back to the loop |
| `espflash reset` — DTR/RTS into the ROM downloader and back out | loop continues |
| reflash, `--flash-freq 20mhz` (`clock div:4` confirmed by the ROM) | loop continues |

Board `A0:F2:62:86:7E:44` showed the identical loop with the identical
`Saved PC` for an hour, through four flashes and two different images — and
booted perfectly the moment Yona **unplugged it** to move hub ports. A power
cycle is the only reset that reaches the LP domain.

## Why the flash stub also fails, and why `--no-stub` does not

espflash 3.3.0's RAM stub times out connecting to a board in this state
(`espflash::timeout`, ~5 s), while the same command with `--no-stub` connects
immediately and reads the chip correctly. That is the same root cause seen from
the other side: the stub raises the CPU/flash clock, which goes through the
same BBPLL/REGI2C path, and wedges the same way. `--no-stub` stays on the
crystal and never touches the analog master — which is why
`espflash board-info --no-stub` worked all evening on a board that could not
boot. **Every script under `scripts/emu/` therefore passes `--no-stub`**, and
`espflash erase-flash` (stub-only) is unavailable on a wedged board.

## What wedges it in the first place

Not proven. The best-supported reading:

- Both boards of the fixture entered this state while their **UART0 pads were
  cross-wired** (D6/GPIO16 ↔ D7/GPIO17, GND joined) with each board powered from
  its **own** USB port. Cross-driven pads between two independently-powered
  boards push current through the receiving pad's ESD clamp into its 3V3 rail
  whenever one side is in reset, unpowered, or still ramping — and every reset
  of either board is then a supply transient on the other.
- A transient across the analog LDO/reference during the bootloader's BBPLL
  programming window is exactly what leaves a REGI2C transaction with `BUSY`
  stuck, and the LP domain then keeps it stuck across every subsequent reset.
- Board `A0:F2:62:87:49:A0` is currently wedged with the **wires already
  removed**, which does not contradict this: it was healthy (running Seeed's
  factory demo) after the unwiring, and went into the loop during the
  flash-then-`hard-reset` that followed. Once wedged, it stays wedged.

## What to do

1. **Power-cycle the board** — unplug the USB cable and plug it back in. That is
   the whole cure, and it is the prediction this record stands or falls on:
   `A0:F2:62:87:49:A0` should boot the shipped image cleanly afterwards.
2. **After any `--no-stub` flash on this fixture, power-cycle rather than
   trusting `--after hard-reset`.** A hard reset does not clear the LP domain.
3. Before re-wiring the two boards: power both from the **same** hub so their
   rails come up and go down together, and put **~330 Ω in series** in each of
   the two signal wires. That is the standard cure for cross-driven pads between
   independently-powered boards, and it costs nothing at 115,200 baud.
4. The wiring itself still wants checking against the pinout card while the
   wires are off: on the XIAO ESP32-C6, **D6 = GPIO16 = U0TXD** and
   **D7 = GPIO17 = U0RXD**, and the pair must **cross**.
