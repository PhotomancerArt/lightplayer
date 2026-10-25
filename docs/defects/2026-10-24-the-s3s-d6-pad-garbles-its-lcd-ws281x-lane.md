---
status: open
found: 2026-10-24      # how: hardware-walk (the S3's first real-strip test of LCD_CAM, FS3, Yona at the desk)
area: lp-fw/fw-esp32s3 LCD_CAM WS281x output (src/output/lcd/) × the XIAO ESP32-S3 Plus's D6 pad (GPIO43, UART0 TX)
class: misattributed-symptom   # read as a GPIO43/UART0 fault; the pad itself carries a clean frame on silicon (2026-10-24 probe)
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (strip-test/report.md)
  - docs/adr/2026-10-22-s3-parallel-ws281x-output.md
  - lp-core/lpc-hardware/boards/seeed/xiao-esp32-s3-plus.json
---
# The S3's D6 pad garbles its LCD WS281x lane

**Symptom** — FS3 (XIAO ESP32-S3 Plus, 16 MB), release 2026.10.24-4
(`249d93a29`, LCD_CAM the default driver), five 8-LED strips on D6, D7, D8,
D9 and D10. Every strip but D6's showed the pattern exactly as the emulator
decodes it. D6's was "a flickery mess": most of the flicker on the first two
LEDs, data sometimes reaching past them, "a lot like non-ws281x data bursts
being interpreted by the chips". It stayed that way with D6 the only output
open (lane 0), and across a chip reset. The same strip moved to D5 was clean
at once, and D5 then carried lane 3 (D6's lane in the five-strip project)
cleanly beside the other four. So the fault follows the pad, not the strip,
the lane, or the lane count. D7 (GPIO44, UART0 RX) is fine.

**Emulator** — `lp-cli emu run --chip esp32s3` on the same image and project
decodes a byte-exact frame off GPIO43 (`LCD_DATA_0`, 0 errors), so no
emulated walk can catch this.

**Root cause** — not in the firmware or the chip: the pad carries a clean
frame on silicon (the probe below). What is left is the path from the pad to
the strip: the D6 joint or wire (the strip was re-soldered when it moved to
D5, and D5 was clean at once), or D6's output stage under load. Telling those
apart needs hands; see "Next".

**The probe (2026-10-24, FS3, same session's desk sitting)** — an image with
`ws281x_pad_probe` (new, never shipped) and `ws281x_frame_check`, built from
`f3c9ecfb3` plus the probe, flashed over release 2026.10.24-4 and put back
after. The project ran D6, D7, D8, D9 and D10 (five 8-LED lanes; D6 lane 2).
D6 had no strip on it (Yona had moved it to D5). Over 3,000 frames:

- GPIO43's registers were what the lane's bind set, on every frame, and the
  same as the other four pads': `IO_MUX_GPIO43_REG` = `0x1a00` (MCU_SEL 1 =
  GPIO, FUN_IE on, FUN_DRV 2, no pulls); `GPIO_FUNC43_OUT_SEL_CFG_REG` =
  `0x087` (signal 135 = `LCD_DATA_2`, OEN_SEL 0); output enabled; `GPIO_PIN43`
  0 (push-pull). `changes=0`: nothing re-selected U0TXD or touched the pad.
- UART0's `TX_DONE` raw interrupt, cleared at bind and read before every
  frame, never set (`uart_tx_frames=0`): nothing in the image, ROM printf and
  the radio blobs included, sent a byte on UART0.
- The pad read low at every frame's start (`high_at_start=0`), as the bus
  leaves it after the latch.
- The CAM-half frame check, which samples the pad's own input buffer, decoded
  GPIO43 byte-exact with the bit widths in window: 3,250 checked, 0 failed,
  0 skipped. So the waveform is right **at the pad**, not only in the matrix.

Evidence: `lp2025/2026-10-20-2335-s3-radio-and-ota/strip-test/d6-probe/`
(`probe-d6.txt` the run, `probe-boot.txt` the Skirt boot, `kg-first.txt` and
`kg-last.txt` the known-good release before and after).

What the strip-test session knew, before the probe:

- The firmware's console is USB-Serial-JTAG (`esp-println` `jtag-serial`); no
  S3 code opens UART0.
- esp-hal 1.1.1's `connect_peripheral_to_output` sets the pad's IO_MUX
  function to GPIO and routes the lane through the GPIO matrix, so on paper
  U0TXD no longer reaches the pad once the lane is bound.
- Leads then, both ruled out by the probe: UART0 TX getting back onto the
  pad at runtime (the ROM's `printf` channel, the radio blobs), and GPIO43's
  pad defaults differing from the other pads'.

**Workaround** — wire WS281x strips to D0–D5 and D7–D10, not D6.

**Fix** — none in the firmware, and the board definition keeps offering D6:
nothing on the chip side is wrong with it.

**Next** — one hands check closes it. Put a strip back on D6 with a fresh
joint (or check continuity and the joint from the D6 pad to the strip's DIN)
and look again on release 2026.10.24-4. Clean: the cause was the joint;
close this `fixed` with that. Still garbled with a good joint: flash the probe
image (`just fw-esp32s3-split esp32s3,server,float-f32,ws281x_pad_probe,ws281x_frame_check`,
`app.bin` at `0x10000`) and read `[ws281x-check]` with the strip on. Failures
there mean D6's output stage cannot drive the load (raise FUN_DRV, or refuse
D6 in `xiao-esp32-s3-plus.json` for this board); none means the fault is past
the pad, on the board or the wire.

**Regression coverage** — none needed in the emulator: silicon and the
emulated S3 agree on GPIO43 at the pad. `ws281x_pad_probe` stays in the tree
as the desk tool for the next pad that misbehaves.

**Lesson** — a pad the board definition offers for an output is not proven
until real data has crossed it, and when one pad fails, sample that pad on
the chip before blaming it. UART0's TX pad looked like the cause because the
ROM drives it before the firmware runs; the registers and the pad's own input
said otherwise in one sitting. The frame check samples the pad, so it splits
"the chip sends it wrong" from "the wire loses it" with no analyzer.
