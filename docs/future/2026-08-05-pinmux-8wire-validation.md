# Eight Wires, Fully Validated — and the Fadecandy Scenario

## Status

Future work, and part of the story LightPlayer wants to tell. Captured
2026-08-05, the night the wire/slot pool shipped for the Zook dome demo
(branch `claude/zook-5wire-pinmux`). Companion piece:
`2026-08-05-s3-dual-core-ws281x-overlap.md` (the S3 side).

## Where validation stands

The classic-ESP32 driver now pools at most four two-block RMT transmitters
behind N declared wires, rebinding slot→pad per transmission through the GPIO
matrix. **The design target is eight wires; validation is tiered:**

| tier | state |
|---|---|
| 4 wires | **Fully validated**: telemetry (zero trips/skips/errors, 170 s runs, cap 4 overlapped) + visual on the DOM-Z-102's four fused/level-shifted DATA terminals |
| 5 wires | Telemetry-validated on the dome rig; visual = the Zook demo (IO13 spare terminal, raw 3.3 V pad — no level shifter, adequate on the bench) |
| 6–8 wires | **Designed, untested.** No hardware: the DOM-Z-102 exposes five data-capable terminals; an 8-strip rig is a *different* esp32v3 board with more ports |

## What "fully testing 8" needs

- **The 8-pad board + rig.** Bench-hardware lead time; the board exists as a
  product family question, not just a test fixture.
- **Wave-schedule telemetry at 8**: two full waves of four; the per-slot
  `[WS281X]` counters aggregate across wires that share a slot, so an 8-wire
  run also wants the per-wire attribution layer. **Landed 2026-08-05** (the
  overlap plan's P4): `[WS281X-WIRE]` lines carry per-wire posted / sent /
  torn (slot-delta attribution) / waved (the second-wave signature) /
  aborted / cancelled / failed / worst post→completion latency.
- **Mux-correctness proof beyond visual**: an RMT-RX loopback harness (route
  a muxed pad's signal back into a receiver via the matrix) proving the right
  bytes leave the right pad through takeovers. Optional for 5 (visual + trip
  counters carry it); worth building for 8.
- **The total-LED budget shipped as manifest soft limits** — heap binds
  (~1500–2000 LEDs today) before frame time does; "8 channels" must never be
  read as 8× dome strips.

## The Fadecandy scenario (why 8×64 is the realer target than 8×300)

Fadecandy's magic was never the port count — it was **temporal dithering at
high refresh over short strips** (8×64), interpolating between host frames so
color depth looks continuous. RIP Fadecandy; thanks for the dithering logic.

LightPlayer already carries `dithering_enabled` in the display pipeline, and
the arithmetic now lines up: at 8×64 = 512 LEDs the engine tick shrinks far
below dome scale and wire time is ~1.9 ms/wave — the refresh ceiling goes to
hundreds of Hz, which is where temporal dithering actually pays (~400 Hz for
smooth results). A specific dev board is earmarked for trying exactly this.
That experiment closes the loop: dome-scale (5×300, ~30 fps) and
Fadecandy-scale (8×64, high-refresh dithered) as the two ends of one
architecture's envelope.

## And the S3

Same pool architecture, own cap measurement, PSRAM invariant — see the
companion doc. The 8-wire target carries over; deciding its queueing belongs
to the P7 planning run (`_archive/2026-08-04-1845-dualcore-rmt-isr/
p7-pinmux-pooled-slots.md` — the constraint capture, including the 8-wire
direction and its two conditions).
