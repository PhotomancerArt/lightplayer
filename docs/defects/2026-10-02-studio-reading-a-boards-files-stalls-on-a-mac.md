---
status: fixed
found: 2026-10-02      # how: hardware-walk (G1 of the C6 repartition, Yona's Chrome, the spare XIAO C6)
fixed: 650299682
area: lpa-link providers/browser_serial_esp32 (browser_esp32_flash.js — every flash read)
class: assumed-context
related:
  - docs/defects/2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md
  - docs/defects/2026-10-02-the-emulated-serial-path-never-drops-a-byte.md
  - docs/defects/2026-10-02-the-host-filesystem-read-throws-away-in-flight-packets.md
  - lp2025/2026-10-01-1843-c6-repartition (G1-F2)
---
# Studio's "Reading the board's files" stalls on a Mac: Web Serial drops bytes of erased flash

**Symptom** — G1 of the C6 repartition, Yona's Chrome on macOS, the spare
XIAO C6 (`10:bd:a3:b0:8e:30`, old layout, 27 files, 47 of 240 blocks):
Update firmware connected, uploaded the stub, read the partition table
("Reading the board's layout"), then "Reading the board's files" repeated
×12 and the card sat at "Flashing firmware… · 5%". No console errors while
anyone watched. Nothing was written. `just walk-migration-emu` passes the
same step.

Reproduced on silicon, 2026-10-02, the executor itself
(`inspectLayout`, the same JS module Studio serves) in headless Brave over
real Web Serial to that board: the 960 KB read stopped at 310,706 bytes
(115 packets), the Transport's buffer had 3.5 KB of a packet it would never
finish, no write was pending, and 100 s later esptool-js threw
`No serial data received.` from `Transport.read` — esptool-js's per-packet
timeout, which is where Yona's "indefinitely" ends if you wait. The same
module over a bridge that reads the tty with plain termios (no browser
serial stack) read all 960 KB in 4.8 s, and so did real Studio over that
bridge (`inspect` to the layout question in ~4 s). The bytes were lost
between the board and the page, on the Mac's side.

**Root cause** — the 2026-09-26 macOS Web Serial loss, met by a new kind of
traffic. Chromium opens the port with `PARMRK` set, so xnu's tty queues a
data `0xFF` twice in its 1,024 slots and `IOSerialBSDClient`'s unsigned
free-space count wraps once doubling passes 1,020; from then on the tty
drops what the page has not read. The device link was routed around it
(lp-link keeps `0xFF` off the wire), but esptool-js's `readFlash` streams
raw flash: 4 KB packets, 1024 asked for in flight, and a C6's filesystem
region is mostly erased sectors — nothing but `0xFF`. One 4 KB packet of
erased flash is 8 KB of tty slots, so the first time the page reads late a
run of bytes vanishes mid-packet. esptool-js then waits for the rest of a
packet the stub believes it sent, the stub waits for the ack, and nothing
moves until the 100 s timeout. The assumption that broke: that the serial
path delivers what the board sends — true on Linux, true over `lp-cli`'s
termios, true in the emulator, false on a Mac for `0xFF`.

**Fix** — `browser_esp32_flash.js` stops using esptool-js's `readFlash`.
Every read (the layout inspection, the migration's verify, the filesystem
backup, the boot-control readback) goes through `readFlashSafely`: the
same stub `READ_FLASH` command, but 384-byte packets with one in flight,
so the most a packet can occupy is 770 tty slots (every data byte costs at
most two, plus the SLIP delimiters) — under what the tty holds with no
reader at all, and the stub sends nothing more until the page has read the
packet and acked it. Each packet's length is checked as it arrives and the
stub's closing MD5 digest is compared with the bytes (esptool-js read and
discarded neither), so a read that loses anything fails in 3 s with a
message instead of stalling. Cost: a round trip per 384 bytes — the
960 KB read took 14.2 s on the desk C6 (21.7 s at 256-byte packets), where
esptool-js's took 4.8 s on a lossless path.

**Regression coverage** — `lp-app/lpa-link/tests/js/mac_tty_model.test.mjs`
(`just lpa-link-js-test`, CI `validate-browser`): a fake stub reads mostly
erased flash through `MacTtyModel`; esptool-js's read parameters lose
bytes, `readFlashSafely` reads it byte for byte with one packet in flight,
and a short packet or a wrong digest fails. The emulator walk now models
the loss on a Mac (`docs/defects/2026-10-02-the-emulated-serial-path-never-drops-a-byte.md`).
Silicon (2026-10-02, headless Brave over real Web Serial, macOS, the spare
C6; Studio shown only that board's port): the executor alone, before —
stalled at 310,706 of 983,040 bytes — and after — 983,040 bytes MD5-checked
in 14.2 s; real Studio, before — "Flashing firmware…" for 100 s, then
`Serial data stream stopped` and no question — and after — the layout
question 14 s after Update with the right count ("27 files (107 KB)"), then,
continued, the whole migration: the board came back on the new layout
(47 of 176 blocks) with all 27 files byte-identical, its project rendering.
The board was then restored to its old layout and firmware from a full
4 MiB image.

**Lesson** — routing one protocol around a host bug does not retire the
bug: every other byte stream on the same path still meets it. The 09-26
entry's lesson was "make the link end to end reliable regardless of what
any one hop does"; esptool-js is a second protocol on that hop with no
retransmission and no way to notice a loss, and the bytes it carries
(erased flash) are the worst case for this exact bug. Anything new that
talks over Web Serial on a Mac — a flasher, a dumper, a probe — needs
either `0xFF` off the wire or a window small enough that the tty cannot
overflow.
