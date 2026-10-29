---
status: fixed
found: 2026-10-28      # how: user report (Yona plugging a blank XIAO ESP32-C6 into a Mac for the flower; Studio in the browser over Web Serial)
fixed: 2026-10-28
area: lpa-devices Identify activity (`activity/identify.rs`, `Device::spawn_identify`)
class: assumed-context   # the reset assumed the C6 keeps its USB through a reset; this board does not
related:
  - docs/defects/2026-10-25-a-blank-board-opened-mid-boot-loop-offered-every-board.md
  - docs/defects/2026-10-19-a-re-enumerating-board-remounts-its-card.md
  - docs/adr/2026-10-28-install-identifies-the-board-first.md
  - docs/defects/2026-10-28-the-emulated-c6-keeps-its-usb-through-a-reset-a-blank-xiao-does-not.md
---
# A blank board that re-enumerates on reset sat on "Identifying" forever

**Symptom** — a new, blank XIAO ESP32-C6 (`303a:1001`, MAC
`10:BD:A3:B0:8E:30`, desk mark LC6) plugged into a Mac. Studio's card said
"Identifying" for as long as the board stayed plugged in, and never offered
Install.

**Cause** — since the 2026-10-25 fix, identify answered a blank verdict
with no chip heard by resetting the board once (`RunReset(Normal)`) so its
ROM banner would name the chip, and kept listening instead of settling.
That fix assumed the C6 keeps its USB through the reset. On this board it
does not. Measured from a host on 2026-10-28:

- Studio's `D0 W100 R1 W100 R0` pulse took the port away (`Device not
  configured`, `/dev/cu.usbmodem2101` gone) within ~250 ms, 3 of 3 tries;
- the blank board's own boot loop (`rst:0x7 (TG0_WDT_HPSYS)`) also drops
  USB every few seconds with nobody touching it.

So the reset closed the link before the banner could be read (the ROM
prints it while the port is gone). The identify ended Interrupted, the
port came back as a new pending device with no record and no memory of the
reset, its first `invalid header` line sent the reset again, and so on for
as long as the board stayed plugged in. The 10-25 fix was walked on the
emulated C6 only, and the emulator's USB shim rules that a C6 keeps its USB
through a reset, so the walk could not see it
(docs/defects/2026-10-28-the-emulated-c6-keeps-its-usb-through-a-reset-a-blank-xiao-does-not.md).

**Fix** — the banner reset is gone. A blank verdict settles at its first
`invalid header` line again and hands the port back, the race against the
watchdog the 2026-08-30 bench settled. Nothing needs the banner's chip
before Install since #1320: Install reads the chip off the ROM's SYNC
reply in the bootloader session (`FlashBuildChoice::ByChip`), and the card
learns it there. Before Install, the Hardware row says the chip is
unknown when no banner was heard.

**Regression test** — `lpa-devices` `tests/scenarios.rs`:
`a_blank_board_that_drops_off_usb_settles_blank_every_time_and_is_never_reset`
(six attach → mid-loop blank line → drop cycles; each settles blank at its
first line and none sends a reset). It fails on the 10-25 code.

**Lesson** — a reset that "costs nothing" on a board with no firmware
still costs the link when the board re-enumerates, and a model that
forgets per link cannot remember it already tried.
