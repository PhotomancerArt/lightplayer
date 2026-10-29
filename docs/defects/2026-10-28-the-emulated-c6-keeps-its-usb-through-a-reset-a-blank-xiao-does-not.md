---
status: open
found: 2026-10-28      # how: tracing a stuck "Identifying" on a blank XIAO ESP32-C6 (LC6, 10:BD:A3:B0:8E:30) from a Mac host
area: Studio's emulated serial shim (`?emu=`, `?emu=tab`; the re-enumeration ruling in `scripts/emu/walk-reenumerate-emu.mjs`), lp-emu-esp32c6 USB-Serial-JTAG
class: fidelity
related:
  - docs/defects/2026-10-28-a-blank-board-that-re-enumerates-on-reset-identified-forever.md
  - docs/defects/2026-10-19-a-re-enumerating-board-remounts-its-card.md
---
# The emulated C6 keeps its USB through a reset; a blank XIAO does not

**What the emulator does** — the shim rules that a C6's USB-Serial-JTAG
survives a reset the serial channel asks for, so an emulated board keeps
its port through the card's Reset pulse and through its own boot-loop
watchdog. Re-enumeration exists only when a walk injects it by hand
(`walk-reenumerate-emu`).

**What silicon did** — a blank XIAO ESP32-C6 (erased flash, ROM boot loop),
read from a Mac with a host that leaves DTR/RTS alone, on 2026-10-28:

- Studio's Normal reset (`D0 W100 R1 W100 R0`) took the port away
  (`Device not configured`, the `/dev/cu.usbmodem*` node gone ~0.5 s)
  within ~250 ms, 3 of 3 tries;
- with nobody touching it, the boot loop's `rst:0x7 (TG0_WDT_HPSYS)`
  resets took the port away every few seconds as well (polled at 2 Hz: gone
  at about 6 s intervals; not every watchdog reset dropped it).

**Not yet known** — whether a C6 running LightPlayer does the same on
the Reset pulse (the 10-25 defect cites `rst:0x15 (USB_UART_HPSYS)` and no
drop, unattributed), and whether other C6 boards behave like this XIAO.
One desk check on a running board settles which rule the shim should have.

**Cost of the gap** — the 10-25 banner-reset fix was walked on the
emulator only and looped forever on this board
(docs/defects/2026-10-28-a-blank-board-that-re-enumerates-on-reset-identified-forever.md).
