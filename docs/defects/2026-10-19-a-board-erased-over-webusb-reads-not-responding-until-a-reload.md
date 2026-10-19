---
status: fixed
found: 2026-10-19      # how: the phone USB walk's Factory reset (Pixel 8a, Android 14, Chrome 155, loose-c6 on the phone's port); then Yona's install walk on a laptop, a real C6 over Web Serial
fixed: this change     # PR #1198
area: lpa-devices (`Device::status`, the record's firmware label, the Erase activity); lpa-link `browser_esp32_flash.js` (`eraseDeviceFlash`); lpa-studio-core board card (Factory reset's place, the Firmware details)
class: state-conflation
related:
  - docs/defects/2026-10-18-a-board-that-never-spoke-offered-retry-not-install.md
  - docs/defects/2026-10-18-a-flash-over-webusb-on-a-phone-loses-the-boards-replies.md
  - docs/adr/2026-10-18-android-usb-over-webusb.md
---
# A board Studio erased reads "not responding" until a reload or a replug

(Filed as "A board erased over WebUSB reads 'not responding' until a
reload"; the laptop walk showed it is not WebUSB's.)

**Symptom** — over Chrome on Android's WebUSB port, Factory reset from the
card erases the board (esptool: "Chip erase completed successfully in
2.978s"; the ROM then loops "invalid header: 0xffffffff"), and the same
page's card then reads "USB · not responding", "Firmware unknown · no
response", and offers Retry, not Install firmware. Reloading the page fixes
it: the granted port reconnects with no chooser, and the card offers
Install ("This board doesn't have LightPlayer firmware", or "This board
doesn't respond" — both with Install).

On a laptop over Web Serial (Yona, a real C6, 2026-10-19), the same dead
end, in his words: after the reset, "I hit the 'Retry' button and it times
out 'This board stopped responding over USB.'", and "Firmware says 'No
response — try flashing firmware'". Only unplugging and replugging the
board reached the blank state: "it's basically a dead-end which isn't
great." He also looked for Factory reset under Hardware; it was in the
Firmware row's details.

**Root cause** — two halves, either enough for the dead end:

- *Studio forgot what it had just done.* A board "stops responding" only if
  it once said hello (`Device::spoke_light_player`, PR #1195), and the
  record keeps the firmware label of the last hello it heard, "never
  cleared". The erase removed that firmware but not the label, so a silent
  erased board read as a LightPlayer that went quiet: NotResponding, the
  surface's "stopped responding", Retry. Retry re-ran identify, which heard
  the same silence. The model held two facts that disagreed — "this board
  spoke LightPlayer" and "Studio just wiped it" — and only the first was
  stored.
- *On Web Serial the erased chip said nothing at all.* `eraseDeviceFlash`
  in the browser flasher closed the port without resetting the chip, unlike
  every other flasher call there (`loader.after("hard_reset")`) and the
  host's erase (`reset_into_app`). The chip stayed in esptool's stub,
  silent, until a replug power-cycled it into its blank boot loop. The
  Erase activity's own doc assumed "esptool's closing reset just rebooted
  the (now blank) board". On the phone the ROM did loop, but its lines never
  reached the link that did the erase (see "What was known" below), so the
  card was left with the same silence.

**Fix** —

- A successful Erase is Studio's own knowledge: the device keeps an
  `erased` fact (`Device::known_blank`), cleared by any firmware verdict (a
  hello, a core, other firmware) and by a flash or update that ends well,
  and the erase clears the record's firmware label. A silent erased board
  is the blank chip — face `Blank`, `NeedsAttention`, no Retry — through
  silence, a re-identify and a re-enumeration, and the card says "This
  board doesn't have LightPlayer firmware." with Install firmware.
- The browser erase resets the chip after the wipe (a failed reset is a
  warning: the wipe is done and the model already knows the board is
  blank). The Erase settles the moment it hears the boot loop, and gives
  the port back whenever it ends well, as identify does on a blank verdict.
- Factory reset moved to the Hardware row's details (danger, beside
  Forget); same offer path, same rules.
- The Firmware details no longer repeat a needs-firmware verdict as a
  dashed label ("No response — try flashing firmware"); the card's top says
  it, the row says it in its own words.

**Regression tests** — `lpa-devices` `tests/scenarios.rs`:
`a_factory_reset_board_that_says_nothing_is_blank_not_unresponsive`,
`a_factory_reset_settles_on_the_boot_loop_and_gives_the_port_back`.
`lpa-studio-core` `board_card_tests.rs`:
`a_factory_reset_from_hardware_lands_on_install_firmware_with_no_retry`
(presses Factory reset by path from Hardware on a board the fake's
`fake_erase_without_reset` leaves silent; without the fix it reads "This
board stopped responding over USB." / "Retry to reconnect.").

**What was known** (the phone, before the fix) — the ROM's "invalid header"
lines never reached the fold on the link that did the erase, so `classify`
never said `Blank`. A fresh link (the reload) did see them. A lab page that
opened the port and pulsed RTS (DTR low) read them at once. Still not known:
whether the board printed them while no reader was open (between esptool's
disconnect and the controller's reopen, and a C6 drops what nobody reads),
or the reopen over WebUSB does not reset the board the way it does over Web
Serial. The fix does not depend on the answer.

**Lesson** — when Studio itself changes what is on a board, that change is
a fact the model stores, not something it waits for the board to confirm.
A board that cannot speak (blank, parked, re-enumerating) never will.
