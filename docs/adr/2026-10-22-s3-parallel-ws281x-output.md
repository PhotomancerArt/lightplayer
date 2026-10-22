# ADR: Parallel WS281x output on the ESP32-S3 (LCD_CAM i8080 + GDMA)

- **Status:** Accepted for the backend, behind the `lcd-ws281x` feature.
  **Making it the S3's default is pending Yona's ruling**, which waits on the
  costs and a real 5-strip test (below).
- **Date:** 2026-10-22
- **Plan:** `lp2025/2026-10-20-2335-s3-radio-and-ota`, milestone M9; evidence
  from its LED-channels spike (`led-channels-spike/report.md`)
- **Relates to:** `2026-07-31-lp-ws281x-multi-channel-driver-adoption.md`
  (a second backend shape under the same seam);
  `2026-08-02-classic-hli-refill.md` (its "argued alternative", realised here);
  `2026-08-04-concurrent-ws281x-flush.md` (`background_tx_safe`)

## Context

The S3's WS281x output was four RMT channels.

- **Each channel is refilled by an interrupt with a 30 µs deadline.** One
  48-word block halves into one LED.
- **The S3 driver overrides neither `start` nor `background_tx_safe`,** so
  channels transmit one after another with the CPU spinning: 18.6 ms per
  frame at 620 LEDs.
- **A fifth channel does not exist.** The S3's RMT has 4 TX channels. The
  glow skirt wants 5, and so does the Zook dome.
- **A frame in flight dies during a flash write.** esp-storage holds
  `rsil 5` around every ROM flash call
  (`docs/adr/2026-10-21-s3-radio-memory.md`), so the refill interrupt cannot
  run. The classic measures the same family even with its RMT pusher on the
  APP core: "one torn frame per write burst"
  (`2026-08-05-ws281x-transmission-on-app-core.md`).

The spike compared five options (RMT + SPI, RMT DMA, LCD_CAM, SPI octal,
core 1 + dedicated GPIO):

- RMT DMA exists only on TX channel 3.
- SPI2 octal caps one transaction at 32 KiB.
- Core 1 would spend a whole core, and it conflicts with esp-storage's
  multicore strategy.

LCD_CAM was the one with no deadline and up to 16 lanes. On FS3 it decoded
**252 frames with 0 lane errors**, including **156 with the CPU in a flash
erase and write**, and 32 with the CPU masked for the whole frame. The check
was the CAM half of the same peripheral sampling the pads.

## Decision

1. **Add an LCD_CAM i8080 + GDMA backend** under the existing
   `lpc-hardware::Ws281xDriver`/`Ws281xOutput` seam:
   `fw-esp32s3/src/output/lcd/`.
   - The lanes are the bits of an 8-bit bus at 2.5 MHz.
   - A WS281x bit is three 400 ns slots: T0H 400, T1H 800, period 1200 ns.
   - The encoding is `lp_ws281x::parallel`, host-tested: a transpose, GRB on
     the wire.
2. **Lockstep is a property.** All lanes clock out together, a frame lasts as
   long as its longest lane, and short lanes are held low (Yona's ruling).
3. **A frame goes out when every open lane has started,** or when a lane
   starts again or waits. It is encoded in one pass into the buffer not in
   flight.
   - `background_tx_safe()` is **true**: no CPU is involved between start
     and latch.
4. **The frame buffers are internal RAM, by capability:**
   `alloc_caps(Internal)`, never default placement.
   - Never `dram2_seg` (untagged; the bootloader takes it back on a warm
     reset).
   - Never PSRAM (GDMA reads PSRAM through the cache, which is off during
     every flash write).
   - Double-buffered, sized by the longest open lane, not by the per-port
     cap. A buffer that cannot be had fails the open with a sentence.
5. **Lanes come from the manifest's `/lcd/ws281xK` resources**, with the
   existing `ws281x-output` capability and no new `HwCapability`.
   - The board schema and any device's `/hardware.json` stay readable by
     today's firmware.
   - Endpoints are the RMT driver's (`ws281x:local:<board label>`), so a
     project opens unchanged on either driver.
6. **Selection is the `lcd-ws281x` cargo feature**, decided in one place
   (`output::register_ws281x_driver`). The RMT driver stays the default.

## Costs (measured)

| | 5 × 124 (skirt) | 8 × 500 |
|---|---|---|
| Frame buffer, one / double | 9,678 / 19,356 B | 36,750 / 73,500 B |
| Wire time (all lanes at once) | 3.87 ms | 14.7 ms |
| Encode | 1.0 ms on the spike's generic path (FS3); fast path: owed | owed |

The image itself, for `lcd-ws281x` against the default: flash +4,676 B
(`.text` +3,844, `.rodata` +832). Internal RAM −2,616 B (the RMT interrupt's
IRAM and `.data`).

## The switch, still to be ruled

The recommendation is **LCD_CAM as the S3's default for every lane count**,
with the RMT left for the `test_loopback` harness and a core-only split
image's status light. Yona rules once these are in front of him:

- RAM per frame (above);
- encode time on silicon;
- frame integrity under Bluetooth + Wi‑Fi scan + flash writes on the product
  image;
- a real 5-strip test.

What would flip it: a board mixing WS2811 and WS2812 timing on one bus (the
3-slot plan has one timing), or a board that cannot spare the buffer at 1–2
lanes.

## Consequences

- **Two WS281x backends on the S3**, one chosen at build time.
  - The S3 emulator models GDMA and LCD_CAM in a parallel lane, so a
    `lcd-ws281x` image can boot in CI.
  - Until then only silicon exercises it.
- **The encoder knows no peripheral.** The C6's PARL_IO and the classic's
  I2S0 LCD mode (clean-room from the TRM, never NeoPixelBus) are follow-ups
  on the same design.
- **The 16-bit bus (16 lanes) and per-lane timing are unbuilt.**
