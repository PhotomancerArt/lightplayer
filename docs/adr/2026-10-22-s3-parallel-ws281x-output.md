# ADR: Parallel WS281x output on the ESP32-S3 (LCD_CAM i8080 + GDMA)

- **Status:** Accepted. **The S3's default WS281x driver since Yona's ruling
  of 2026-10-22** ("yes", on the costs below), landed after PSRAM went on by
  default and freed the internal RAM its frame buffers need (see "Found on the
  product image"). The real 5-strip test is owed to a desk sitting.
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
   (`output::register_ws281x_driver`). It is in the S3's `default` features
   (amended 2026-10-23); an image built without it registers the RMT driver,
   which the `test_loopback` harness and a core-only split image's status
   light still use.

## Costs (measured on FS3, 2026-10-22)

| | 5 × 124 (skirt) | 8 × 500 |
|---|---|---|
| Frame buffer, one / double | 9,678 / 19,356 B | 36,750 / 73,500 B |
| Wire time (all lanes at once) | 3,880 µs measured (3,871 predicted) | 14,708 µs measured (14,700 predicted) |
| Encode (the fast path, 240 MHz, `s`) | **651 µs** (729 first frame); 886 µs at 8 × 124 | **3,564 µs** |
| Encode, worst, beside Bluetooth + Wi‑Fi scans | 5,471 µs (radio pre-empting the encode) | — |

The image itself, `lcd-ws281x` against the default:
- flash +4,676 B (`.text` +3,844, `.rodata` +832);
- internal RAM −2,616 B (the RMT interrupt's IRAM and `.data`).

Frame integrity, all on FS3 through the CAM-half analyzer:
- 5 lanes, 8 × 124 and **8 × 500**, CPU masked, **5 frames per run with the
  CPU in a flash erase and write**, and a 1,000-frame soak: all byte-exact
  (`RESULT pass`).
- **Beside Bluetooth (16 connections) and back-to-back Wi‑Fi scans (75):**
  6,250 frames, no output error seen. 17 frames were only partly checked: the
  analyzer's window closed early because radio work delayed the LCD start by
  0.6–1.5 ms; every captured bit and width was correct.

### Found on the product image: where the buffer's RAM comes from

On the Bluetooth image playing a 2 × 310-LED project, **the backend could not
get even one 23,070 B buffer of `Internal`-tagged RAM.** Free heap was
77,836 B and the largest block 55,504 B, but that space is in `dram2_seg`.
`dram2_seg` is untagged so that no DMA lands there, and the engine fills the
tagged arena first. The open failed with its sentence, as designed. The same
project plays on the RMT image.

So the switch needs RAM the tagged regions do not have today. Two ways to get
it:

- **M10 (PSRAM adoption)** moves engine RAM out of the arena.
- **A ruling on `dram2_seg` for TX-only DMA.** The warm-reset hazard that
  bars `dram2_seg` is DMA *writing* into the bootloader's RAM. LCD GDMA only
  *reads* there, but the rule today is "never".

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
