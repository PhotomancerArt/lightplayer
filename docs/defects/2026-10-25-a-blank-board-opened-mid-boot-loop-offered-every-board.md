---
status: fixed
found: 2026-10-25      # how: user report (Yona setting up a blank ESP32-C6 from his phone in Studio, Chrome for Android over WebUSB)
area: lpa-devices Identify activity (`activity/identify.rs`, `Device::spawn_identify`); lpa-studio-core install picker (`flash_offer`, `board_pick_offer`)
class: assumed-context   # the chip fact assumed the port was open when the ROM printed its banner
related:
  - docs/defects/2026-10-19-a-board-erased-over-webusb-reads-not-responding-until-a-reload.md
  - docs/defects/2026-10-19-a-re-enumerating-board-remounts-its-card.md
  - docs/adr/2026-10-18-android-usb-over-webusb.md
---
# A blank board opened mid boot loop offered every board kind

**Symptom** — Yona plugged a blank ESP32-C6 into his phone and pressed
Install firmware in Studio. The board picker listed every board kind
Studio serves (classic, S3 and C6 boards alike) instead of the C6's, and
the picker said "No boot banner named the chip…".

**Cause** — the install picker narrows by chip (`flash_offer(detected_chip)`),
and on a board with no firmware the only source of the chip is the ROM's
boot banner, `ESP-ROM:esp32c6-…`, which `Evidence` reads off the
board's lines. The ROM prints that banner once per boot, first. A blank
chip then floods `invalid header: 0xffffffff` until its watchdog resets it.
Identify settles a blank verdict on the first `invalid header` line and
hands the port back at once (bench 2026-08-30: waiting out the settle
window loses the race against the watchdog). So a port that opened mid
loop, which the phone's WebUSB grant always did, heard the flood and never
the banner: the chip stayed unknown and nothing narrowed the list. On a
laptop the port usually opened in time to catch a banner, which is why it
hid.

**Fix** — a blank verdict with no chip heard resets the board once
(`RunReset(Normal)`, the card's Reset pulse) instead of settling, and keeps
listening: the ROM prints its banner, the chip is read, and the loop's next
`invalid header` settles the blank verdict with the port handed back. Once
per identify run: a banner lost anyway (a phone drops bytes) settles on the
next blank line with no chip, a reset the port refuses settles at once, and
the deadline settles a board that went quiet. It is never sent to a board
that said hello (only the blank verdict asks for it), to a device whose
record already knows its chip, or over a link with no reset lines (a
network link). The board has no firmware, so the reset costs nothing, and
the C6's own reset keeps its USB (it does not re-enumerate; the ROM prints
`rst:0x15 (USB_UART_HPSYS)`). It runs through `browser_serial.js` on both
Web Serial and the WebUSB port (`usb_cdc_serial.js` implements
`setSignals`), so the phone gets it with no WebUSB-specific code.

**Regression tests** — `lpa-devices` `activity/identify.rs`:
`a_blank_board_with_no_banner_is_reset_once_for_it`,
`the_banner_reset_is_sent_once_and_never_waited_on`,
`a_chip_already_named_or_a_reset_not_allowed_settles_at_once`.
`lpa-studio-core` `device_flash_offer.rs`:
`a_blank_c6_opened_mid_boot_loop_gets_a_c6_only_pick_once_its_banner_answers`
(the pending link's Flash offer lists the C6's boards only, every other
board behind `all_boards`).

**Not yet walked on the phone** — the emulator walk covers the reset on an
emulated C6; the Pixel walk with a blank C6 is owed.

**Lesson** — a fact that is printed once, at a moment Studio does not
control, is not something to wait for. When it is cheap to make the board
say it again, ask.
