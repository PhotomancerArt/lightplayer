---
status: open
found: 2026-09-26      # live-debugging (a prod recording), then a soak on silicon
area: host serial path (Chromium Web Serial on macOS × xnu tty × IOSerialFamily) × the packed wire encoding
class: assumed-context
related:
  - docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md
  - docs/adr/2026-09-24-json-pack-wire-encoding.md
  - ~/.photomancer/planning/lp2025/2026-09-26-1720-reliable-device-link/reports/m1-loss.md
---
# Web Serial on macOS drops bytes of packed frames whenever the page reads late

**Symptom.** Studio on prod (2026-09-26, recording
`20260926-164522-419728501bdda564.jsonl`, board `10:bd:a3:b0:a5:2c` on gated
firmware) saw 11 torn packed frames in ~2.5 minutes of editing: `expected
project read frame seq 0, got 1`, one kick back to Devices, and a read that
never ended (Save stopped working). The board's link counters were all zero
and it logged nothing: every write succeeded on its side. Each torn frame
lost a run of bytes that starts ~1.2–1.5 KB into the frame, partway through
a 64-byte USB packet, and resumes at a packet boundary.

**Root cause (reproduced on silicon, code read in the two open-source
components).** It is the host, and only the macOS Web Serial path:

1. Chromium opens the port with `PARMRK` set and `IGNBRK` clear
   (`services/device/serial/serial_io_handler_posix.cc`, `ConfigurePortImpl`),
   so that a break or parity error arrives in-band as `0xFF 0x00 x`. With
   `PARMRK` set, a data byte `0xFF` is delivered as `0xFF 0xFF`, which
   Chromium's `CheckReceiveError` folds back.
2. Those flags keep IOSerialFamily from bypassing the line discipline, so
   each byte goes through xnu `ttyinput` (`bsd/kern/tty.c`), which queues a
   data `0xFF` **twice** — two of the tty queue's 1,024 slots for one byte —
   and silently drops any byte that arrives with the queue at `MAX_INPUT`
   (no flow control: no `IXOFF`, no `CRTS_IFLOW`).
3. `IOSerialBSDClient::getData` feeds the tty at most
   `TTY_HIGHWATER - (rawq + canq)` bytes per pass (1,020 minus what is
   queued), counting one slot per byte. The difference lands in a `UInt32`:
   once doubling has pushed the queue past 1,020 it wraps, `MIN(…, 1024)`
   turns it into a full kilobyte, and the "no room, block" test (`<= 0` on an
   unsigned) never fires. Every later pass hands the full tty another
   kilobyte to drop, until the reader drains it.

So: whenever the page's reader falls behind by about a kilobyte and a half
(the tty's 1,024 slots plus Chromium's 255-byte pipe), packed frames lose
bytes, in runs of up to a kilobyte a pass. JSON text contains no `0xFF`, its
queue never passes 1,020, and IOSerialBSDClient's byte count is exact
backpressure: JSON is never lost this way, which is why JSON lost 0 of 1,270
lines on 2026-09-24 while packed lost frames on the same board and afternoon.
The learned-table packed encoding is full of `0xFF` (end-of-object markers,
the COBS code byte every 254 bytes): ~1 % of its bytes.

**Evidence** (silicon `10:bd:a3:b0:8e:30`, `soak_link` image, macOS; full
tables in the plan's `reports/m1-loss.md`):

| reader | encoding | bytes | lost |
|---|---|---:|---|
| `lp-cli link soak` (raw termios), no stalls / 1.5 s stalls every 3 s | packed | 4.3 MB each | **0** |
| headless Brave Web Serial, `bufferSize` 255 | packed | 2.85 MB | 15 torn frames, 15,705 B |
| headless Brave Web Serial, `bufferSize` 65,536 | packed | 2.63 MB | 15 torn, 24,771 B |
| headless Brave Web Serial, `bufferSize` 255 | **JSON** | 3.09 MB | **0** |
| native reader with **Chromium's termios**, no stalls, no browser | packed | 8.68 MB | 93 torn, 67,926 B |
| the same | JSON | 8.98 MB | **0** |
| the same plus `IGNBRK` (no doubling, bypass allowed) | packed | 2.84 MB | **0** |
| cfmakeraw plus `PARMRK` only | packed | 2.74 MB | 20 torn, 24,333 B |

**Fix** — not applied. Options, none in firmware: (a) keep `0xFF` off the wire
(an encoding change: an escape, or a framing that excludes 0xFF as COBS
excludes 0x00) — makes packed as safe as JSON on this path; (b) a link layer
with retransmission (the plan this was found in); (c) report upstream to
Chromium (open the port with `IGNBRK` when parity is off, or clear `PARMRK`
unless parity checking is on) and to Apple (the unsigned free-space count).
Studio cannot set termios itself. Larger `bufferSize` does not help (table).

**Stopgap (2026-09-27)** — Studio asks boards for JSON, not packed replies, on
real Web Serial when the browser runs on macOS
(`lp-app/lpa-studio-web/src/wire_encoding_default.rs`; decision 3 at G1 of the
investigation). JSON never carries `0xFF`, so this path loses nothing (table
above). Other OSes, `?emu=` pages (the emulator's `navigator.serial` shim has no
tty), Bluetooth (never packed) and `lp-cli` (native termios) are unchanged, and
`?wire=packed` turns packing back on for a measurement. It goes when the device
link moves onto `lp-link` (ADR `docs/adr/2026-09-27-lp-link-one-comms-layer.md`),
whose COBS-FF framing keeps `0xFF` off the wire. The cost meanwhile is JSON's
size on a Mac: the wire bytes JSON Pack saves (ADR 2026-09-24) are spent again.

**Stopgap removed (2026-09-27, plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`,
P4, decision D14)** — Studio's Web Serial link is lp-link (`WIRE_PROTO_VERSION`
30): every byte on the wire is a COBS-FF frame or raw boot text, neither of
which carries `0xFF` (a panic's text mark is the one deliberate `0xFF`, and it
is written before any frame), and a frame that is lost or damaged anyway is
resent. So a page on a Mac asks for packed replies again, like every other
page; `?wire=json` remains the dev override. `wire_encoding_default.rs` is
deleted. The Chromium/Apple bug is untouched — lp-link routes around it.

**Regression coverage** — none in CI (it needs macOS and a board).
`scripts/link/tty-soak.py --termios chrome` reproduces it on any macOS host
with a `soak_link` board; `scripts/link/mac-tty-model.py` replays any
capture through a model of the path and shows the same packed-only loss.

**Lesson** — "the OS serial path is a transparent byte pipe" was an
assumption, and it held only for bytes that are never `0xFF`. A byte-level
encoding change is also a change to what the host's line discipline sees.
The board-side IN-endpoint gate (2026-09-24 defect) was a real fix for a
real board-side loss, and it made this one look like a regression.
