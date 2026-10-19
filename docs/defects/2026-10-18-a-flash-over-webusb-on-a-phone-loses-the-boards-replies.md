---
status: open
found: 2026-10-18      # how: firmware installs from Chrome on Android (Pixel 8a, Android 14, Chrome 155) to loose-c6 (XIAO ESP32-C6) on the phone's USB-C port, traced at the WebUSB port and in a bare lab page
area: lpa-studio-web `public/lpa-link/usb_cdc_serial.js`; lpa-link `browser_esp32_flash.js` (esptool-js 0.6.0 over the C6's USB-Serial-JTAG)
class: backend-contract-divergence
related:
  - docs/adr/2026-10-18-android-usb-over-webusb.md
  - docs/defects/2026-10-18-a-failed-reopen-kept-the-ports-writer-and-fed-the-flasher.md
---
# A flash over WebUSB on a phone loses the board's replies

**Symptom** — over Chrome on Android's WebUSB port, esptool gets through the
reset, sync, chip detection, the MAC, the stub upload and megabytes of
flash reads, then waits forever for a frame that never finishes: "No
serial data received". Studio's card said "install failed".

**What was measured** (a lab page on the phone driving esptool-js over the
shipped port, reads only):

- With one IN transfer at a time, the stub's first `c0` arrived and its
  "OHAI" did not. With 16 queued, the stub started.
- A continuous 4 KB stub packet lost about 40 % of its bytes whether 1 or 16
  transfers were queued (2,396 to 2,587 of 4,096 arrived). So queueing more
  transfers did not raise what the phone reads.
- Ack-paced 384-byte packets read 64 KB of the app image in about 1 s with
  nothing lost, and 1.4 MB of erased flash; the same packets over the
  board's JSON files stopped within 300 to 1,200 bytes, at a different
  point each run, and never resumed (a 20 s wait). The stub's output there
  is many short packets: the C6's ROM output flushes at every `0x0a`, and
  the stub at every `0xc0` and every 63 bytes.
- What arrives is in order; what is lost is lost at the chip, which drops
  output the host is not reading (Espressif's USB-Serial-JTAG does this by
  design, esp-idf#11192 for the application driver).

**Root cause (likely, not proven)** — the phone reads the C6's IN endpoint
more slowly than the chip writes, and the chip drops rather than waits.
esptool's protocol has no resend, so one lost byte ends the session.
Whether the slowness is Chrome on Android serializing WebUSB transfers, or
Android's USB host, was not determined. lp-link has a resend, so the link
itself (connect, edit, Play, the over-the-air update) is unaffected.

**Now** — the flasher is refused over WebUSB: `browser_esp32_flash.js`'s
`isSupported` is false there, the provider advertises no flash, and the
card says "Installing firmware from a phone needs a computer" before
anyone presses (`LinkInfo::firmware_blocked`).

**To fix** — a flash that survives loss: reads that resume from the last
good packet after a stall (a fresh stub connection costs about 0.5 s on
the phone), writes that resend a block whose reply was lost, or a
slower-paced protocol. Then lift the refusal, walked on the phone.
