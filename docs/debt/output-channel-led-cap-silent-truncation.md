---
status: carried
since: 2026-05-18
logged: 2026-07-31
area: lp-fw/fw-esp32-common/src/output/provider.rs + lp-fw/fw-esp32c6/src/output/rmt_ws281x_driver.rs
related:
  - lp-fw/fw-esp32c6/src/output/rmt_ws281x_driver.rs
  - lp-fw/fw-esp32-common/src/output/provider.rs
  - 2026-07-31-0720-s3-led-output-4ch (plan dir, out-of-scope note)
---
# `MAX_LEDS = 256` per-channel output is a silent cap, duplicated in two places

**Shape** — Both `Esp32OutputProvider::open` (the shared provider consumed by
every ESP32 chip) and `fw-esp32c6`'s own RMT driver independently define
`const MAX_LEDS: usize = 256` and run the requested `byte_count` through a
`capped_byte_count()` that `.min()`s it against `MAX_LEDS * 3` with **no log
line, no error, no signal of any kind** when the cap actually bites. A project
authored for a 300-LED strip opens successfully, reports a channel handle, and
silently renders only the first 256 LEDs' worth of data forever — the same
failure *shape* as `docs/defects/2026-07-31-loader-silently-drops-unparseable-nodes.md`
(an operation that "succeeds" while quietly doing less than it was asked),
just one layer up the stack, in code this plan's sweep did not touch.

The two definitions are also a second condition worth naming: `MAX_LEDS` is
transcribed identically in two crates with no shared source of truth, so a
future change to one silently drifts from the other exactly the way
`docs/debt/firmware-partition-constants-transcribed.md` describes for the
partition offset — same mechanism, different constant.

**Carrying cost** — Nothing observed it yet; the desk board's test strips are
8 LEDs and the quad-strip bring-up project (P4) never exercised the cap. The
cost is latent: the first author who wires a long strip (the plan's own
acceptance criteria call out "classic ESP32 will want up to 8" channels, which
raises the odds of longer runs) gets a dim, truncated section of their fixture
with no diagnostic pointing at why, and has to rediscover this constant by
reading driver source.

**Workarounds** — None; keep strip lengths at or under 256 LEDs per channel,
or read `provider.rs` before wiring a long run.

**Incident log**
- **2026-07-31** — Filed during the S3 4-channel output plan's cleanup phase
  (P5) while sweeping the driver code the plan touched. No live incident: the
  cap was already latent in the pre-existing C6 driver and the shared
  provider; filed because the plan's new S3 driver
  (`lp-fw/fw-esp32s3/src/output/rmt/esp32s3_rmt_ws281x_driver.rs`) goes through
  the same `Esp32OutputProvider`, so the same silent cap now also governs four
  channels instead of one.

**Exit criteria** — `capped_byte_count` (or its caller) logs a `warn!` (or
returns a distinguishable error) the moment truncation actually occurs, and
`MAX_LEDS` has exactly one definition the two drivers share instead of two
hand-copied constants.
