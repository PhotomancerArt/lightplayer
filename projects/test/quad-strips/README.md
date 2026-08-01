# Quad strips

The 4-channel WS281x bring-up project for the desk ESP32-S3 (XIAO ESP32-S3
Plus). One shader drives four fixtures, one per output channel, so all four
RMT channels light at once with content that makes wiring and cross-talk
mistakes obvious at a glance.

## Wiring

| Channel | Pin | Bus | Band color |
|---|---|---|---|
| 1 | D10 (GPIO9) | `bus:control.out/ch1` | red |
| 2 | D9 (GPIO8) | `bus:control.out/ch2` | green |
| 3 | D8 (GPIO7) | `bus:control.out/ch3` | blue |
| 4 | D7 (GPIO44) | `bus:control.out/ch4` | amber |

`shader.glsl` renders four horizontal bands, one per output, each with a
distinct base hue and a chase dot at a band-specific speed and phase — a
strip on the wrong pin, or channels bleeding into each other, shows up
immediately as the wrong color or two dots moving together.

## Uploading

```bash
lp-cli upload projects/test/quad-strips serial:auto
```

The device resets on connect and auto-loads whatever is on flash from the
*previous* upload; the freshly uploaded project takes effect on the next
reload/boot. See `docs/defects/2026-07-30-deploy-compiles-previous-upload.md`
for why the first compile line you see after an upload is not this one.
