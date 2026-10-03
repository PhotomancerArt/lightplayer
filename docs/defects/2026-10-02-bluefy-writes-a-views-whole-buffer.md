---
status: fixed
found: 2026-10-02      # how: hardware-walk (Yona, iPhone + Bluefy at lightplayer.app, XIAO C6 bda8 console over USB)
fixed: this change
area: lpa-link browser_ble.js (`write`) × Bluefy's Web Bluetooth
class: backend-contract-divergence
related:
  - docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md
  - docs/defects/2026-10-02-a-bluetooth-reconnect-after-an-unlock-stays-locked-and-flaps.md
  - docs/adr/2026-09-24-ble-transport.md (S3 and its #834 amendment)
---
# Bluefy writes a view's whole buffer, so any request over 512 B drops the link

**Symptom** — on an iPhone in Bluefy, Studio over Bluetooth to a XIAO C6:
pinning a palette made Bluefy put up its native "LP-PLAYFUL disconnected"
alert every time, then reconnect. Knobs worked. With the Mac's Studio on USB
as well, the alerts came one after another, and each one blocked the phone.
The board's console showed the same two lines on every drop:
`[ble] linkN: long write refused at offset 484 (484 B queued): longer than 512 B`,
then `[ble] linkN: disconnected, reason=0x13`. So the phone ended the link,
not the radio and not the board.

**Root cause** — `browser_ble.js` cut each request into 180-byte chunks with
`data.subarray(…)`, which makes views into one buffer.
`writeValueWithResponse` takes a BufferSource, and the standard says to send
the view's bytes. Bluefy sends the view's whole underlying buffer instead, so
every "180-byte chunk" carried the entire request:
- Up to MTU − 3 the whole request arrived once per chunk. The board's line
  joiner turned that into duplicate requests, which did no visible harm.
- Up to 512 B it went as a long write the board reassembles (#834), again
  duplicated.
- Past 512 B the board refused it (`PREPARE_QUEUE_FULL`), the page's write
  failed, and Studio's rule for a failed write tore the link down
  (`gatt.disconnect()`, `0x13`).

Bluefy then reported a drop and Studio reconnected. A palette pin is
comfortably over 512 B; a knob write is not. This is also the unexplained
half of the 2026-09-25 defect ("why Bluefy chose a long write there is not
known"): Studio never asked for one.

Proven on the phone the same day, through `spikes/ble-lab` in Bluefy, with one
650-byte request:
- Cut with `subarray`: the first 180-byte write failed, the page saw a
  651-byte buffer behind it, and the board logged the long-write refusal.
- Cut with `slice`: four writes of 180/180/180/108, all accepted, and the
  board answered.

**Fix** — every chunk is `data.slice(…)`, a buffer of its own.

**Regression coverage** —
`a_long_line_survives_a_browser_that_writes_a_views_whole_buffer`
(`lp-app/lpa-link/tests/browser_ble_conformance.rs`). The `?ble=emu` polyfill
now has a `wholeBufferWrites` switch that models Bluefy: a view's whole buffer
is sent, and past 512 B it is refused. With `subarray` the 701-byte line fails
there; with `slice` it arrives once and whole, in four writes.

**Lesson** — a typed-array view is not a byte string to every browser.
Anything handed across a browser API boundary should own its buffer,
especially in a wrapper browser (Bluefy, WebBLE) whose bridge serialises
`.buffer`. The earlier defect looked at the board and the packet sizes; the
answer was in the page's chunking, a layer nobody suspected because a
standard browser behaves.
