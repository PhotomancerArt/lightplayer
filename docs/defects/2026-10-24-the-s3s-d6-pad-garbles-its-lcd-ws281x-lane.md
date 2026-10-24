---
status: open
found: 2026-10-24      # how: hardware-walk (the S3's first real-strip test of LCD_CAM, FS3, Yona at the desk)
area: lp-fw/fw-esp32s3 LCD_CAM WS281x output (src/output/lcd/) × the XIAO ESP32-S3 Plus's D6 pad (GPIO43, UART0 TX)
class: fidelity        # the emulated S3 decodes a clean frame off GPIO43; silicon does not
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

**Root cause** — not found. What is known:

- The firmware's console is USB-Serial-JTAG (`esp-println` `jtag-serial`); no
  S3 code opens UART0.
- esp-hal 1.1.1's `connect_peripheral_to_output` sets the pad's IO_MUX
  function to GPIO and routes the lane through the GPIO matrix, so on paper
  U0TXD no longer reaches the pad once the lane is bound.
- Leads, in order: something at runtime (the ROM's `printf` channel, which the
  second stage points at UART0, used by the radio blobs; or a ROM/blob call
  that re-selects U0TXD on GPIO43) puts UART0 TX back on the pad between
  frames; GPIO43's pad defaults (drive strength, or a pull the ROM sets for
  UART) differ from the other pads'. A logic analyzer on D6, or reading
  `IO_MUX_GPIO43_REG` and `GPIO_FUNC43_OUT_SEL_CFG_REG` at runtime, tells them
  apart.

**Workaround** — wire WS281x strips to D0–D5 and D7–D10, not D6.

**Fix** — none yet. When the cause is known, either fix it in the driver's
bind, or have the board definition refuse `ws281x` on D6 with the reason.

**Regression coverage** — none: the emulator decodes GPIO43 cleanly. A fix
needs a desk check on D6, and an emulator fidelity change if the cause is
modelable.

**Lesson** — a pad the board definition offers for an output is not proven
until real data has crossed it. UART0's TX pad is the S3's one pad the ROM
and the second stage drive before the firmware runs. Name it in any walk that
checks a pin map.
