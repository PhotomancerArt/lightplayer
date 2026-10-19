---
status: fixed
found: 2026-10-18      # how: the first firmware install from Chrome on Android (the desk's Pixel 8a, loose-c6 on its USB-C port), traced at the port
fixed: 2026-10-18      # PR #1191
area: lpa-studio-web `public/lpa-link/browser_esp32_device_controller.js` (`openForRead`)
class: lifecycle-ownership
related:
  - docs/adr/2026-10-18-android-usb-over-webusb.md
---
# A failed reopen kept the port's writer, and the link's frames went into the flasher

**Symptom** — a firmware install from the phone read 1.4 MB of the board's
flash over the esptool stub and then hung: the stub stopped answering, and
the flash failed with esptool's "No serial data received". A trace of every
USB write showed lp-link frames (`00 02 02 07 …`, COBS-delimited) going to
the board in the middle of esptool's SLIP traffic, thousands of them.

**Root cause** — the device controller's `openForRead` took the port's
writer before its reader. Mid-flash, Studio's link reopened its session (an
access read asked the link for the board): `openPortWithRetry` saw the port
already open (esptool had it), the writer was free between esptool's writes
so the controller took it, and then `getReader()` failed on the reader
esptool held. The open failed, but the writer stayed taken, and the link
wrote its frames through it into the stub's input. The stub skips bytes
outside a SLIP frame, so it survived until a frame carrying `0xC0` broke one
and the stub missed an ack.

The same `openForRead` runs on Web Serial, so the same race was possible on
a desktop; it was found on the phone because a flash there takes longer.

**Fix** — the reader first, then the writer, and a failed open releases
what it took: a session that cannot read a port holds nothing on it. The
WebUSB port also refuses `close()` while a stream is held, as Web Serial
does. Pinned by `lp-app/lpa-link/tests/js/usb_cdc_serial.test.mjs` ("a
session reopening while esptool holds the port fails and holds nothing"),
which fails on the old order.
