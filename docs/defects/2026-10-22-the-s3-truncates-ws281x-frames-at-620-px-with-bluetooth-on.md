---
status: open
found: 2026-10-22      # how: hardware-walk (the S3 PSRAM spike's control image, FS3)
area: lp-fw/fw-esp32s3 output/rmt (lp-ws281x refill on the S3, four channels, Bluetooth on)
class: deadline-margin-by-accident
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (psram-spike/report.md §1, §3, §7; m9-s3-parallel-led-output.md)
  - docs/defects/2026-10-01-the-c6-choker-truncates-most-ws281x-frames-on-silicon.md
  - docs/adr/2026-08-05-ws281x-transmission-on-app-core.md
  - docs/adr/2026-07-31-lp-ws281x-multi-channel-driver-adoption.md
---
# The S3 truncates about 3 % of WS281x frames at 620 pixels on four outputs with Bluetooth advertising

**Symptom** — FS3 (`fixture-s3`, XIAO ESP32-S3 Plus, D8:3B:DA:47:29:70) ran
the shipped S3 defaults on 2026-10-22: Bluetooth on and advertising, no
central connected, no PSRAM, plain image. It rendered a 620-pixel project on
four RMT outputs: 4 × 155 LEDs on D0/D1/D3/D10, the zook-dome shader on a
32×20 render, 22 fps. The driver's own counters, summed over the four
channels, after ~35 s:

```text
[psram] … | led frames=3212 trips=109 errors=0
```

109 of 3,212 frames (3.4 %) ended on a guard trip: a refill came too late,
the transmitter stopped early, and the strip showed a stale or partial tail
for that frame. `errors` (TX underrun) stayed 0. Four PSRAM variants of the
same image read 1.9–2.6 % (`psram-spike/report.md` §1). PSRAM is not the
cause.

The same frame shape **without the app** is clean. The `test_loopback`
harness sends 4 × 155 LEDs concurrently from the same ISR and driver, with no
Bluetooth and no server. It read 0 trips in every 40-frame row (400 frames),
including rows where core 0 thrashed PSRAM, core 0 streamed code from flash,
or core 1 thrashed PSRAM. The worst refill entry delay there was 4/9 of the
24-word deadline (7/9 with core 1 on the bus).

Provenance: silicon, not emulated. The image is the spike branch
`claude/spike-s3-psram-6a95bf` at `8b0b25385`, feature `psram-spike` with
`LP_PSRAM_POLICY=off`. That is `main` at `ccfa116b0` plus two spike changes:

- a heartbeat line that reads `DRIVER.stats` (this chip has no
  `ws281x_telemetry`);
- the spike's esp-alloc fork change (region cap 6, a PSRAM-first threshold
  left at `usize::MAX`).

No PSRAM is brought up. Captures are under
`~/.photomancer/planning/lp2025/2026-10-20-2335-s3-radio-and-ota/psram-spike/captures/`:

- `app-off.console.txt` (this image);
- `app-{init,all,last,split-4096}.console.txt` (the PSRAM variants);
- `led.txt` (the loopback rows).

**Root cause** — not established. The refill ISR runs from IRAM at
`Priority::max()` (level 3). Each 24-word half gives it a 30 µs deadline.
Anything that masks level 3 on core 0 for longer than the remaining margin
truncates the frame. Candidates, none tested:

- **esp-alloc's lock.** Every heap allocation and free holds `rsil 5`
  (esp-sync `NonReentrantMutex`). LLFF first-fit over a fragmented arena
  walks its free list under that lock, and the heartbeat's largest-block
  probe allocates in a binary search. The trip rate was lowest (1.9 %) with
  the whole heap in a fresh 8 MB PSRAM region, where allocations find a block
  at once. That is weak evidence, but it points here.
- **The link's and the runtime's critical sections** (`UsbLinkShared`,
  esp-rtos, embassy).
- **The BTDM controller's interrupts and IRAM handlers** (level 1). These
  should not delay a level-3 ISR unless they mask; whether they do is
  unchecked.

Because the loopback harness is clean and the app is not, the cause is
something the app or the radio does on core 0 while frames are in flight.
The C6's `ws281x_telemetry` lines in the app would name the window: per-trip
`trip_bits_last` and the entry-delay histogram.

**Fix** — none yet. Two shapes remove the race rather than narrow it:

- **M9's LCD_CAM backend** (`m9-s3-parallel-led-output.md`, pending Yona's
  Q-M9a). GDMA feeds the pads from a frame encoded once into internal RAM, so
  there is no per-half refill to win. The LED-channels spike measured 32/32
  clean frames with the CPU masked for the whole frame, and 156/156 during
  flash erases.
- **The internal-copy rule** (PSRAM spike §3), if RMT stays. The ISR's code,
  its state **and the frame it reads** live in internal RAM, so a refill
  never waits on the MSPI bus. This guards against PSRAM-induced lateness
  only; it does not fix whatever masks the ISR here.

**Measured again: Bluetooth connected, two 310-pixel outputs (2026-10-22,
M10's walk, `docs/reports/2026-10-22-s3-psram-walk.md`)** — FS3,
`projects/test/skirt-620` (2 × 310), `fw-esp32s3 --features psram`, which
counts the driver's guard trips (`[led]`). With a central **connected** (Mac
Chrome via CDP, heartbeats only) every heap policy truncated **21.7–27.9 %**
of frames, the heap-internal one (`off`) included (24.8 %, 24.0 %); two runs
whose central had not reconnected, the radio only advertising, truncated
**4.8 %** and **5.1 %**. So the rate grows several-fold with a connection,
and with fewer, longer channels; PSRAM's heap does not move it.

**Regression coverage** — none:

- the S3 app reports guard trips only on a desk build
  (`ws281x_telemetry`'s `[WS281X]` line, off by default; the walk's own
  `[led]` line was folded into it);
- the emulator runs no Bluetooth central and models no interrupt latency
  from the radio;
- the loopback harness covers the driver alone, and it is clean.

**Lesson** — The RMT refill deadline was validated by the loopback harness
and the 1D desk projects, never under the full app with a radio at 620
pixels. As with the C6 choker's truncation, a clean harness says the driver
is right. It does not say the deployed firmware leaves it its margin: that
margin belongs to whatever else masks interrupts on the same core. A design
that needs no margin (DMA from internal RAM) is the durable answer.
