# ADR: On Chrome for Android, Studio's USB square speaks CDC-ACM over WebUSB

- **Status:** Accepted
- **Date:** 2026-10-18
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** none (a side session of the Android Bluetooth work, PR #1180/#1183;
  this change is PR #1191)

## Context

Studio reaches a wired board through Web Serial (`navigator.serial`), in
`lpa-link`'s browser serial provider and the device controller under it
(`browser_esp32_device_controller.js`). On the desk's Pixel 8a (Android 14,
Chrome 155), pressing USB with a C6 on the phone's USB-C port showed Chrome's
serial chooser saying "No compatible devices found", even with an unfiltered
`requestPort()`. Android itself saw the board (`dumpsys usb`: host mode, the
C6's CDC-ACM interfaces 0 class 2 and 1 class 10, and interface 2 class 255).

**This is Chrome's limitation, not ours.** Chrome's Web Serial on Android
ships Bluetooth RFCOMM ports only; its intent to ship left wired USB until
Android supports it at the system level. There is no flag or permission that
makes Web Serial list a wired port there today.

`navigator.usb` does reach the board. A WebUSB probe on the phone
(`requestDevice({ filters: [{ vendorId: 0x303a }] })`, `open()`,
`claimInterface(0)` and `(1)`, a CDC `SET_CONTROL_LINE_STATE`, then
`transferIn` on interface 1's bulk IN endpoint) read 630 bytes from the board
in 3 s. Android shows its own "Allow Chrome to access …?" dialog once per
device.

## Decision

1. **A `SerialPort` over WebUSB, written in the repo.**
   `lp-app/lpa-studio-web/public/lpa-link/usb_cdc_serial.js` is a CDC-ACM
   port: `open` (claim the communications and data interfaces,
   `SET_LINE_CODING`, then DTR and RTS asserted together, as a tty's open
   does), `readable`/`writable` byte streams over the bulk endpoints, one
   `SET_CONTROL_LINE_STATE` per `setSignals` (a call naming one line keeps the
   other), `getInfo`, `forget`. Beside it, a `navigator.serial`-shaped object
   over `navigator.usb`: `requestPort` (Web Serial's vendor filters, as WebUSB
   filters that also ask for `classCode: 2`), `getPorts`, and the
   `connect`/`disconnect` edges. It is written from the USB CDC 1.2 and PSTN
   1.2 class specs and the WebUSB spec; Google's Apache-2.0
   web-serial-polyfill was read as a behavioural reference only and nothing is
   copied from it (build tiny and custom, AGENTS.md).
2. **`browser_serial.js` picks the backend, and nothing above it knows.**
   `serialBackendKind()`: a page polyfill (an own `navigator.serial`, as
   `?emu=` installs) always wins; otherwise WebUSB when `navigator.usb` exists
   and the browser is Android (`userAgentData.platform`, else the user agent)
   or has no Web Serial at all; otherwise Web Serial. The device controller,
   the Rust provider and the esptool bridge see a port, as they do under
   `virtual_serial.js` (the rule of
   `docs/adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md`).
   The only change above it is that the controller's chooser takes the
   `navigator.serial` to ask.
3. **The offer does not move.** The USB square is still the one core offer,
   `devices/connect-usb`, with `needs_user_activation` as before. Core reads
   only "is serial supported", which is now true on Android Chrome.
4. **Only CDC-ACM boards are reachable this way.** A CH340, CP210x or FTDI
   bridge speaks its own vendor protocol (the CH340 is interface class 0xFF),
   so a classic ESP32 behind one (DOM-Z-102's CH340K) still needs a computer.
   The chooser lists CDC devices only; opening anything else fails in plain
   words. A bridge that IS CDC (WCH's CH9102, vendor `0x1a86`) is admitted.

5. **Firmware is not written over it, and the card says so first.** The
   phone walk found the flasher losing the board's replies (the C6 drops
   output the phone is not reading fast enough, and esptool's protocol has
   no resend; `docs/defects/2026-10-18-a-flash-over-webusb-on-a-phone-loses-the-boards-replies.md`).
   So `browser_esp32_flash.js` reports flashing unsupported over WebUSB, the
   provider advertises no flash, erase, boot-control or raw-filesystem read
   on such a port, and its `LinkInfo::firmware_blocked` carries
   "Installing firmware from a phone needs a computer". The card draws
   Update firmware, Install firmware and Factory reset disabled with that
   sentence under them, as it does over Bluetooth. The over-the-air update
   rides lp-link's update channel, which resends, and stays offered.

## Consequences

- A phone is a USB host for a C6 with no computer at all. Walked on the
  desk's Pixel 8a (Android 14, Chrome 155) with loose-c6 on its USB-C port
  (`just walk-usb-phone --serve-release`): Chrome's own WebUSB chooser lists
  the board, Android asks once to allow Chrome, and Studio connects,
  identifies the board by MAC, opens the editor and runs Play. Reloading
  reconnects with no chooser (Android keeps the grant). Installing firmware
  from the phone is refused (decision 5).
- The port reads for as long as it is open, whoever is reading (a serial
  driver's buffer, capped at 4 MiB), with 16 packet-long IN transfers in
  flight as desktop CDC drivers keep: a reader that cancels does not take
  an in-flight transfer's bytes with it.
- The device controller now takes a port's reader before its writer, so a
  session that cannot read a port holds nothing on it
  (`docs/defects/2026-10-18-a-failed-reopen-kept-the-ports-writer-and-fed-the-flasher.md`;
  the same open runs on Web Serial).
- The open asserts DTR and RTS together, never RTS alone: RTS asserted while
  DTR is not resets a C6. The controller's reset dances then run as they do on
  a Mac, each step a whole-status write; the conformance suite pins that the
  scripted board decodes exactly one reset from them.
- Tests: `lp-app/lpa-link/tests/js/usb_cdc_serial.test.mjs` (the port against
  a C6-shaped `USBDevice` double, under the real device controller;
  `just lpa-link-js-test`), and four tests in
  `tests/browser_serial_conformance.rs` that route the scripted boards over
  WebUSB and bring the Rust link up through the shipped JS
  (`just lpa-link-browser-test`). The phone walk is `just walk-usb-phone`.
- When Chrome on Android ships wired Web Serial, the backend rule's Android
  arm can go; the port stays useful for any browser with WebUSB and no Web
  Serial. Lifting decision 5 needs a flash that survives loss (the open
  defect says what was measured), walked on the phone.

## Amendment 2026-10-19: decision 5 is lifted — firmware is written over WebUSB

The flasher now survives what a C6 drops on a phone, so the refusal is
gone. The card offers Update firmware, Install firmware and Factory reset
over a phone's USB as over a computer's, and `LinkInfo::firmware_blocked`
and `FIRMWARE_NEEDS_A_COMPUTER` are removed (nothing produced them any
more).

**What it took.** A lab page driving esptool-js over the shipped port
found that the loss is narrow: the stub ends a USB packet at every `0x0a`,
and the phone falls behind a long burst of short ones. Writes, whose
replies are one short frame with one command waiting, lost nothing. And
the stub recovers a read without a new session: acking a READ_FLASH's
whole length ends its send loop in about 60 ms. So the fix is the client
side alone, in `browser_esp32_flash.js`. esptool-js, Espressif's stub and
the firmware are unchanged:

- reads are READ_FLASH segments, each checked against the stub's own
  digest and read again with half-length packets when one is lost (clean
  segments double them again; WebUSB 32 to 384 B, Web Serial 64 to 384 B
  starting at its 384);
- each written image is proven by FLASH_MD5 and written again from its
  FLASH_DEFL_BEGIN when it does not match or a reply is lost. A block is
  never resent, because the stub would inflate it twice;
- up to three bootloader sessions over WebUSB.

Writing a stub of our own, or installing through the over-the-air update,
was weighed and not needed. The over-the-air update already works from
the phone, but a blank board, a board that needs USB once and the files
migration have no LightPlayer firmware to speak lp-link, so the ROM and
stub path had to work.

**Walked** on the desk's Pixel 8a (Android 14, Chrome 155) with loose-c6
on its port, `just walk-usb-phone --serve-release`, which now writes
firmware:

- the USB update of a fielded board, littlefs to the tree store with its
  files moved and its project kept: 2026.10.10-14 to 2026.10.19-2 in 55 s;
- a first install onto a blank board in 9.3 s, the board's hello on the
  new build, then a project, the editor and Play;
- Factory reset (esptool's erase, 3.0 s): the board is blank after it.
  The card that erased it says "not responding" until the page reloads
  (`docs/defects/2026-10-19-a-board-erased-over-webusb-reads-not-responding-until-a-reload.md`,
  open); the walk reloads.

No write or bootloader session was retried on those runs. Read segments
read twice were not logged then; `readFlashSafely` now says each one on the
flash's terminal, and the walk counts them. The node tests
(`lp-app/lpa-link/tests/js/browser_esp32_flash.test.mjs`) lose packets,
digests and replies on purpose.
