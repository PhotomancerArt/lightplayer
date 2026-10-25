# Skirt 620 — a 620-pixel, two-channel measurement rig

The S3's load at the skirt's target size
([`docs/reports/2026-10-22-s3-620px-ble-load.md`](../../../docs/reports/2026-10-22-s3-620px-ble-load.md)):
two fixtures of 10 strands × 31 lamps = **620 lamps**, one WS281x output per
side of the skirt (`D10`, `D9` — GPIO9 and GPIO8 on the XIAO ESP32-S3 Plus),
both sampling [`../basic`](../basic)'s noise shader at 32 × 32 (Direct).

`clock.json`, `shader.json`, `shader.glsl` and the outputs are `basic-2n`'s;
only the fixtures' maps (a 10 × 31 grid each, side by side on a 100 × 100
canvas) and render size differ. It is shaped like a skirt's wiring and load,
not the skirt's own pattern: change the shader or the render size and the
memory figures in the report move with it.

```bash
lp-cli emu run --chip esp32s3 --host-link --merged <chip.bin> --upload projects/test/skirt-620 --timeout 24s
lp-cli upload projects/test/skirt-620 serial:<port>      # on a board
```
