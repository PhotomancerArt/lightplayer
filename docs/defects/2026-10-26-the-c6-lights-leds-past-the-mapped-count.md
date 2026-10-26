---
status: fixed
fixed: this change     # host-proven against MockRmt; silicon confirmation owed (see Fix)
found: 2026-10-26      # how: report (Yona, playful-choker-tryout on a XIAO C6, Wi-Fi on)
area: lp-fw/lp-ws281x (driver start of frame) + lp-fw/fw-esp32c6 output/rmt
class: deadline-margin-by-accident
related:
  - docs/debt/c6-scan-truncation-accepted.md
  - docs/defects/2026-09-02-c6-ws281x-first-three-leds-then-stale.md
---
# ESP32-C6: two or three LEDs past the mapped count light up and freeze

**Symptom** — XIAO ESP32-C6 running the playful-choker tryout (73 mapped
pixels on D10) with Wi-Fi on, more LEDs wired after the mapped ones. Within
about a minute of boot two LEDs past the last mapped pixel light up, later a
third, and hold real pattern colours from then on. The output sends exactly
the mapped count.

**Cause** — `lp-ws281x`'s `start_frame` prefills both halves of the RMT
window and plants no guard word (the guard the steady state relies on comes
with the first refill). Its module docs named the cost: a lost first
interrupt "replays the initial buffer once". It is worse than once: the
transmitter goes round the window for as long as the first refill interrupt
is late, and the re-sent bits are inserted into the stream, so the frame's
last bits spill one half further down the chain per half of delay. On the
two-channel plan a half is 24 bits — one pixel, 30 µs — and Wi-Fi after boot
holds the interrupt off for that long and longer. The mapped pixels show one
shifted frame and recover; the LEDs past them are never addressed again, so
they keep the spilled colours. Three frozen LEDs means a first interrupt at
least 90 µs late. Nothing counted it: the cursor reaches the frame's end, so
it was not a guard trip.

**Fix** — in the driver, invisible above it:

- the first refill checks where the read pointer is; back in the first half
  means it wrapped, counted in `ChannelStats::replays` (an even number of
  replayed halves looks like an on-time service and is not seen);
- `Ws281xDriver::enable_replay_recovery` (on for every C6 output channel,
  `REPLAY_TAIL_PIXELS = 8`) follows each frame that can replay with black
  pixels — at least a whole window — so a spill lands in black and the next
  frame rewrites it black again; and at `tx_end` of a transmission that
  replayed, the handler sends the same frame once more before publishing
  completion (`ChannelStats::resends`), so the shifted frame lasts one
  transmission. The C6's `[WS281X]` stats line now ends `replays= resends=`.

`lp-fw/lp-ws281x/tests/replay.rs` reproduces it on a model strip: without
recovery the LED past the frame freezes on the last pixel's colour; with it,
every LED the tail reaches is black after the event.

Owed on silicon: the C6 board, the playful-choker project, trailing LEDs
attached, Wi-Fi on; boot a few times and watch the LEDs past the 73rd and
the `replays=`/`resends=` columns. A spill longer than the tail (a first
interrupt more than ~270 µs late) would still freeze the LEDs beyond it.

**Lesson** — a documented "replays once" was a guess about a race whose
length is set by someone else's interrupt latency. A DMA backend (PARLIO or
SPI on the C6) has no refill deadline and would retire this with the
truncation debt.
