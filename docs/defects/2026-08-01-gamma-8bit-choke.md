---
status: fixed
found: 2026-08-01      # how: diagnosing the classic-ESP32 OOM; noticed dim projects rendering binary pixels
fixed: 2026-08-01      # 16-bit interpolated gamma LUT in the fixture node
area: lp-core/lpc-engine fixture node
class: precision-loss-at-a-seam
related:
  - lp-core/lpc-engine/src/nodes/fixture/gamma.rs
  - lp-core/lpc-shared/src/display_pipeline/lut.rs
---
# 8-bit gamma choke in the 16-bit render pipeline

**Symptom** — The render pipeline is unorm16 end to end: shader
`read_sample_out` returns `Vec<u16>` → brightness applied in u16 → gamma →
power limit in u16 → `Unorm16` control product → `DisplayPipeline`
("16-bit in, 8-bit out", interpolation + temporal dithering at the wire).
But the gamma step truncated to 8 bits internally:

```rust
r = apply_gamma((r >> 8) as u8).to_q32().to_u16_saturating();
```

`apply_gamma` indexed the canonical Adafruit `GAMMA8: [u8; 256]` table —
exactly `round(255·(i/255)^2.8)`, zero error at best-fit γ=2.8 — a
survivor from before the pipeline went 16-bit.

**Measured consequences** (2026-08-01):

- Only **163 distinct output levels** survive the u8 round-trip (the u8
  gamma output skips codes when expanded back to u16).
- The bottom **28/256 of the input range collapses to hard 0**
  (GAMMA8[0..=27] = 0).
- At fixture brightness 38/255 — the real desk project quad-strips-v3 —
  full white lands on GAMMA8[38] = 1, so the entire project's post-gamma
  range was **{0, 257}: binary pixels**.
- The 16-bit temporal dithering downstream cannot recover information
  gamma already destroyed.

**Fix** — `apply_gamma16(u16) -> u16`: a 513-entry `const` LUT
(`[u32; 513]`, 16.2 fixed point, ~2 KB .rodata, shared by every fixture on
every chip; no heap, no per-channel storage) evaluated by linear
interpolation — the same shape as the white-point LUT in
`display_pipeline/lut.rs`. γ=2.8 kept for visual continuity with the
legacy table. Max error vs the analytic curve < 1 count in 65535, monotone,
exact endpoints; all asserted by tests in `gamma.rs`, including a
regression test for the brightness-38 case (318 distinct levels, was 2).
The table is a checked-in literal guarded by an exact regeneration test,
so hand-edits are impossible.

Ordering unchanged (load-bearing): brightness → gamma → power scale →
color order. Power scale must come after gamma — see `power_limit`.

**Deliberate output change** — any fixture with `gamma_correction=true`
(the engine default when the project doesn't specify) renders differently
— that is the point. The S3 device-vs-host bit-exact comparison shifts on
both sides together and stays bit-exact.
