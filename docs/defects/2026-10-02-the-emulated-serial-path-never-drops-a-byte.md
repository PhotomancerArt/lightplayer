---
status: fixed
found: 2026-10-02      # how: hardware-walk (G1-F2 disagreed with `just walk-migration-emu`)
fixed: this change
area: lpa-studio-web public/lpa-link (virtual_serial.js — the emulator's navigator.serial shim)
class: fidelity
related:
  - docs/defects/2026-10-02-studio-reading-a-boards-files-stalls-on-a-mac.md
  - docs/defects/2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md
  - docs/adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md
---
# The emulated serial path never drops a byte; a Mac's does

**Symptom** — G1-F2: Studio's layout inspection read stalled on a real
C6 on Yona's Mac and passed every run of `just walk-migration-emu`, on the
same Mac, against the same firmware layout.

**Root cause** — the emulator lane's `navigator.serial` shim
(`virtual_serial.js`) hands the page every byte an emulated board sends,
in order, into an unbounded stream queue, however late the page reads.
That is a lossless pipe, which is what Linux and `lp-cli`'s termios are.
Chromium's Web Serial on macOS is not: with `PARMRK` set, xnu's tty counts
a data `0xFF` twice against its 1,024 slots and `IOSerialBSDClient`'s
free-space count wraps past 1,020, so `0xFF`-heavy bytes the page reads
late are dropped (`docs/defects/2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md`).
The shim's own header names the behaviours of the real API it keeps
because "a polyfill that improved on them would make the emulator LESS
faithful than a board"; this was a third one, and it improved on it. The
09-26 loss had been routed around for the device link (lp-link keeps
`0xFF` off the wire), so nothing the walks exercised carried `0xFF` until
esptool-js started reading erased flash.

**Fix** — `mac_tty_model.js`: the tty's 1,024 slots, `0xFF` costing two,
`IOSerialBSDClient`'s wrapping `1020 - queued`, unbounded backpressure for
everything else, and Chromium's 255-byte pipe. With `hostTty: "mac"`, the
shim's readable is pulled through the model — a pipe's worth per read — and
counts what it drops on the port (`ttyDropped`, plus a console warning per
drop). `index.html` turns it on when the page runs on a Mac (`?emu-tty=mac`
or `?emu-tty=none` overrides), so the desk's walks see the desk's path; CI's
Linux runners and the conformance suite keep the lossless pipe. Bytes that
are not `0xFF` are never dropped by the model, so the lp-link traffic every
walk carries is unaffected.

**Regression coverage** — `lp-app/lpa-link/tests/js/mac_tty_model.test.mjs`
(`just lpa-link-js-test`): non-`0xFF` bytes survive any lateness, a 4 KB
`0xFF` burst read late loses bytes, a burst that fits doubled does not,
esptool-js's read parameters lose erased flash through the model and
`readFlashSafely` does not. The model's numbers are the 09-26 defect's,
measured on silicon then; the model has not been replayed against a fresh
capture.

**Lesson** — the emulator lane's host side is a stand-in too. A shim that
is more reliable than the browser it replaces hides exactly the class of
bug that only the browser's own path has, and the parity rule ("fix the
emulator when the board disagrees") applies to the page's half of the
cable as much as to the chip's.
