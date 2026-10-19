---
status: fixed
found: 2026-10-19      # how: the phone USB walk's Factory reset (Pixel 8a, Android 14, Chrome 155, loose-c6 on the phone's port)
area: lpa-link `browser_esp32_flash.js` (the browser erase), on every backend; found over WebUSB
class: backend-contract-divergence
related:
  - docs/defects/2026-10-18-a-flash-over-webusb-on-a-phone-loses-the-boards-replies.md
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

**Cause** — the browser's erase (`eraseDeviceFlash`,
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

The reload "fixed" it only because a fresh page decides again with no
activity behind it. The lab page that pulsed RTS (DTR low) read the ROM's
lines at once, because the pulse was the missing reset.

**Fix** — `rebootFromFlash` pulses EN before the erase lets go of the port:
RTS asserted with DTR low for 100 ms, then both released, each step one
whole-status write (a CH34x's macOS driver ignores one-line writes, and
the order never passes the CH34x's download-mode selection). The same
pulse through a Mac tty woke the parked CX1 at once (`rst:0x15
(USB_UART_HPSYS)`). Tests: `browser_esp32_flash.test.mjs`, "an erase
reboots the board out of the stub, over WebUSB" (the shipped WebUSB port
over the fake C6: exactly one reset, never the CH34x download state) and
"a board that re-enumerates on the reset does not fail the erase".
`just walk-usb-phone`'s reset step no longer reloads.

**Not yet walked on the phone** — on 2026-10-19 LC6 on the Pixel's port
dropped off the bus every 2.8 s (Android: `Removed device` / `USB device
attached`, with and without the wireless charger), so Chrome's chooser lost
it mid-pick and no walk got past add. The phone walk is owed once the board
holds on the phone's port (a powered hub is on order).

**The flash path, since** — `flashFirmware`, `executePlan`,
`writeBootControl` and `probeTarget` end with the same pulse
(`browser_esp32_flash_reboot.test.mjs` runs each one whole). Before, a
flashed C6 answered only at the reconnect ladder's Normal rung, 8 s after
the write; a classic on a CH340 only at the both-then-drop rung after that,
because its macOS driver ignores the Normal rung's one-line writes.

Was open: whether the board printed while no reader was open, or the
reopen over WebUSB did not reset it the way Web Serial does. Neither: no
backend ever reset it.
