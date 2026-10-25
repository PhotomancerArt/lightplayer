---
status: fixed
found: 2026-10-23      # how: code read + FS3 measurement (the S3 fps spike)
area: lp-fw/fw-esp32-common output provider × lpc-shared display pipeline (lp-fw/fw-esp32-common/src/output/provider.rs)
class: contract-gap    # the pipeline interpolates between ticks; the provider ticks once per write
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (fps-spike/report.md §5)
  - docs/adr/2026-09-06-smoothing-degrades-by-measured-lamp-limits.md
---
# The ESP output showed the previous frame whenever interpolation was on

**Symptom** — every ESP32 board (C6, S3, classic) driving an output with
`interpolation_enabled` on, the authored default (`output_def.rs`; 49
projects in `projects/` and `catalog/` set it true), put the **previous**
frame on the wire, every frame: one frame of latency on every LED, two extra
6 B/LED copies and a blend per frame, and 12 B/LED of buffers held for it. No
test noticed, because a frame late on a moving pattern looks right.

**Cause** — `Esp32OutputProvider::write` called `pipeline.write_frame(0,
data)`, `write_frame(16_667, data)`, then `tick(8_333)`. The second write
rotated the previous frame into `prev` (ts 16,667) and this one into
`current` (ts 0), so `prev_current_delta_us = max(0 − 16,667, 1) = 1`; at the
tick `frame_progress_us = 8,333 − 16,667` saturated to 0, and
`render_interpolated` ran with blend factor 0: `prev × 65,535/65,536`. The
display pipeline interpolates between frames when ticked between them (an
output clock faster than the frame rate); this provider ticks exactly once
per write, so there was never anything to interpolate, only a frame to lose.

**Measured** — FS3 (XIAO ESP32-S3 Plus, 16 MB), 8 × 256 LEDs, LCD_CAM: the
output write 4.8 → 4.0 ms per frame with interpolation off.

**Fix** — the provider opens every pipeline with interpolation off (the
authored flag has nothing to do on these boards) and writes each frame once;
the tick renders the frame just written. Its smoothing notice no longer names
interpolation, which it never provides; `interpolationLeds` sheds nothing on
an ESP board (DOM-Z-102's record stays, harmless). Regression test:
`the_wire_carries_the_frame_just_written` (fails on the old write path with
the dark previous frame).

**Lesson** — a component with a time model (the pipeline's `prev`/`current`
timestamps) driven by a caller that fakes the times can be wrong in a way
no frame-content test sees. A test that writes two different frames and
reads the wire is what pins it.
