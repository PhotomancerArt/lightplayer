---
status: fixed
found: 2026-10-19      # how: the phone USB walk's Factory reset (Pixel 8a, Android 14, Chrome 155, loose-c6 on the phone's port); then Yona's install walk on a laptop, a real C6 over Web Serial
fixed: 7d3a91078 (the reboot, #1197) and this change (the model, #1198)
area: lpa-link `browser_esp32_flash.js` (the browser erase), on every backend; lpa-devices (`Device::status`, the record's firmware label, the Erase activity); lpa-studio-core board card (Factory reset's place, the Firmware details)
class: backend-contract-divergence
related:
  - docs/defects/2026-10-18-a-flash-over-webusb-on-a-phone-loses-the-boards-replies.md
  - docs/defects/2026-10-18-a-board-that-never-spoke-offered-retry-not-install.md
  - docs/defects/2026-10-19-a-re-enumerating-board-remounts-its-card.md
  - docs/adr/2026-10-18-android-usb-over-webusb.md
---
# A board erased over WebUSB reads "not responding" until a reload

**Symptom** — over Chrome on Android's WebUSB port, Factory reset from the
card erases the board (esptool: "Chip erase completed successfully in
2.978s"; the ROM then loops "invalid header: 0xffffffff"), and the same
page's card then reads "USB · not responding", "Firmware unknown · no
response", and offers Retry, not Install firmware. Reloading the page fixes
it: the granted port reconnects with no chooser, and the card offers
Install ("This board doesn't have LightPlayer firmware", or "This board
doesn't respond" — both with Install).

On a laptop over Web Serial (Yona, a real C6, the same day), the same dead
end, in his words: after the reset, "I hit the 'Retry' button and it times
out 'This board stopped responding over USB.'", and "Firmware says 'No
response — try flashing firmware'". Only unplugging and replugging the
board reached the blank state: "it's basically a dead-end which isn't
great." He also looked for Factory reset under Hardware; it was in the
Firmware row's details.

**Cause** — two halves, either enough for the dead end.

*The board said nothing.* The browser's erase (`eraseDeviceFlash`,
`browser_esp32_flash.js`) never reset the board after the wipe, so it stayed
in esptool's stub, which says nothing; the ROM's blank-flash loop that the
fold reads as `Blank` never started. Not WebUSB's doing, and not lines
dropped while no reader was open: on the desk (CX1, 2026-10-19) a C6 parked
in the stub stayed parked through two macOS tty opens and closes (the calls
Web Serial makes on a Mac) and still answered the loader with no reset.
`lp-cli`'s erase resets into the app afterwards (`reset_into_app`); the
Erase activity's comment assumed "esptool's closing reset" had happened.
esptool-js 0.6.0's `after("hard_reset")` would not have done it either: its
HardReset only releases RTS, and the connect's reset sequence already left
both lines low. (The flash path has the same gap, but its reconnect ladder
has a reset rung that covers it; Erase only reopened and listened.)

*Studio forgot what it had just done.* A board "stops responding" only if
it once said hello (`Device::spoke_light_player`, PR #1195), and the record
keeps the firmware label of the last hello it heard, "never cleared". The
erase removed that firmware but not the label, so a silent erased board
read as a LightPlayer that went quiet: NotResponding, the surface's
"stopped responding", Retry. Retry re-ran identify, which heard the same
silence. The model held two facts that disagreed — "this board spoke
LightPlayer" and "Studio just wiped it" — and only the first was stored.

The reload "fixed" it only because a fresh page decides again with no
activity behind it. The lab page that pulsed RTS (DTR low) read the ROM's
lines at once, because the pulse was the missing reset.

**Fix** — the reboot (#1197): `rebootFromFlash` pulses EN before the erase
lets go of the port: RTS asserted with DTR low for 100 ms, then both
released, each step one whole-status write (a CH34x's macOS driver ignores
one-line writes, and the order never passes the CH34x's download-mode
selection). The same pulse through a Mac tty woke the parked CX1 at once
(`rst:0x15 (USB_UART_HPSYS)`).

The model and the card (#1198):

- A successful Erase is Studio's own knowledge: the device keeps an
  `erased` fact (`Device::known_blank`), cleared by any firmware verdict (a
  hello, a core, other firmware) and by a flash or update that ends well,
  and the erase clears the record's firmware label. A silent erased board
  is the blank chip — face `Blank`, `NeedsAttention`, no Retry — through
  silence, a re-identify and a re-enumeration, and the card says "This
  board doesn't have LightPlayer firmware." with Install firmware, whether
  or not the reboot took.
- The Erase settles the moment it hears the boot loop, and gives the port
  back whenever it ends well.
- Factory reset moved to the Hardware row's details (danger, beside
  Forget); same offer path, same rules.
- The Firmware details no longer repeat a needs-firmware verdict as a
  dashed label ("No response — try flashing firmware"); the card's top says
  it, the row says it in its own words.

**Regression tests** — `browser_esp32_flash.test.mjs`: "an erase reboots
the board out of the stub, over WebUSB" (the shipped WebUSB port over the
fake C6: exactly one reset, never the CH34x download state) and "a board
that re-enumerates on the reset does not fail the erase". `lpa-devices`
`tests/scenarios.rs`:
`a_factory_reset_board_that_says_nothing_is_blank_not_unresponsive`,
`a_factory_reset_settles_on_the_boot_loop_and_gives_the_port_back`.
`lpa-studio-core` `board_card_tests.rs`:
`a_factory_reset_from_hardware_lands_on_install_firmware_with_no_retry`
(presses Factory reset by path from Hardware on a board the fake's
`fake_erase_without_reset` leaves silent; without the model half it reads
"This board stopped responding over USB." / "Retry to reconnect."). The
emulator walk: `just walk-factory-reset-emu`. `just walk-usb-phone`'s reset
step no longer reloads.

**Not yet walked on the phone** — on 2026-10-19 LC6 on the Pixel's port
dropped off the bus every 2.8 s (Android: `Removed device` / `USB device
attached`, with and without the wireless charger), so Chrome's chooser lost
it mid-pick and no walk got past add. The phone walk is owed once the board
holds on the phone's port (a powered hub is on order). (An erased C6
re-enumerates on a similar cadence by itself, on its watchdog:
docs/defects/2026-10-19-a-re-enumerating-board-remounts-its-card.md.)

Was open: whether the board printed while no reader was open, or the
reopen over WebUSB did not reset it the way Web Serial does. Neither: no
backend ever reset it.

**Lesson** — when Studio itself changes what is on a board, that change is
a fact the model stores, not something it waits for the board to confirm.
A board that cannot speak (blank, parked, re-enumerating) never will.
