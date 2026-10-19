---
status: open
found: 2026-10-19      # how: the phone USB walk's Factory reset (Pixel 8a, Android 14, Chrome 155, loose-c6 on the phone's port)
area: lpa-studio-web `public/lpa-link/usb_cdc_serial.js` and the device controller's reads after a reset; lpa-devices identification of a just-erased board
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

**What is known** — the ROM's "invalid header" lines never reach the fold
on the link that did the erase, so `classify` never says `Blank`. A fresh
link (the reload) does see them. A lab page that opened the port and pulsed
RTS (DTR low) read them at once. Not yet known: whether the board printed
them while no reader was open (between esptool's disconnect and the
controller's reopen, and a C6 drops what nobody reads), or the controller's
reopen over WebUSB does not reset the board the way it does over Web
Serial. Not checked on Web Serial.

**Two ways out**, either enough: the controller's post-flash reopen over
WebUSB resets the board and reads its ROM again; or the model counts a
successful Erase against the board's old hello, so a just-erased board that
stays silent is offered Install (proposed by the "Offer Install, not
Retry, on a board that never spoke" session, PR #1195's follow-up).

**Meanwhile** — `just walk-usb-phone` reloads Studio after its Factory
reset step.
