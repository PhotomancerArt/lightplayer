---
status: fixed
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

**What was measured next** (2026-10-19, the same lab page driving the
shipped `browser_esp32_flash.js`; Pixel 8a, Android 14, Chrome 155, loose-c6):

- The stub ends a USB packet at every `0x0a` too, not only at `0xc0` and
  every 63 bytes: a 384-byte read packet of the board's JSON files arrived
  as a 1-byte `c0` and then 7- to 21-byte packets, each ending in `0a`,
  about one a millisecond, until one did not come. So what is lost is a
  long burst of short USB packets, and the burst is set by the packet
  size and the data.
- Fixed packet sizes over 32 KB of the files: 32 B at 11 KB/s and 64 B at
  20 KB/s with nothing lost, 128 B at 32 KB/s with one loss, 256 B worse
  than 64 B (11 losses). At 384 B, 512 KB of mostly binary flash lost a
  packet 91 times.
- **A lost packet does not end the stub's session.** Acking the read's
  whole length makes the stub leave its send loop: it sends its digest
  and reads commands again, in about 60 ms. The next READ_FLASH starts
  where the last good packet ended.
- A write's reply was never lost: 512 KB written back over itself three
  times, 2.0 to 2.2 s each, the board's MD5 the same each time. One short
  frame with one command waiting fits the chip's FIFO.
- Ten bootloader sessions in a row all opened, 423 to 507 ms each.

**Fix** — `browser_esp32_flash.js`, client side only (esptool-js and the
stub unchanged):

- `readFlashSafely` reads in READ_FLASH segments, each kept only when the
  stub's MD5 of it matches. A short, broken or late packet, or a lost
  digest, costs the segment: ack its length, drain, read it again with
  packets half as long. Clean segments double them again. Over WebUSB:
  128 B to start, 32 to 384, 250 ms a packet; 512 KB of the board's files
  in 9.1 s, 1 MB of app in 16.6 s. Web Serial keeps 384 B, one in flight
  (the Mac tty rule), and now survives a dropped packet too.
- `writeFlashChecked` proves each image by FLASH_MD5 (asked again if its
  reply is lost) and writes the image again on a mismatch or a lost write
  reply. A block cannot be resent: the stub would inflate it twice.
- Up to three bootloader sessions over WebUSB.
- The refusal is lifted: the card offers Update firmware, Install firmware
  and Factory reset over a phone's USB again (`LinkInfo::firmware_blocked`
  is gone).

Tests: `lp-app/lpa-link/tests/js/browser_esp32_flash.test.mjs` (a fake stub
that loses packets, digests and replies). Walked on the phone with `just
walk-usb-phone --serve-release`, which now updates, factory-resets and
installs firmware: a USB update with the files moved in 55 s, and a first
install onto a blank board in 9.3 s that booted the new build (the ADR's
2026-10-19 amendment).
