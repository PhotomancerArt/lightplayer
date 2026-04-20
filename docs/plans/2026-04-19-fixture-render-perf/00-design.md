# Fixture render perf — design

## Scope of work

Three targeted micro-optimizations to the fixture render hot path,
identified from CPU profiling. Each is small, surgical, and committed
as its own phase so the next profile run can attribute the speedup to a
specific change.

1. **u32 multiply in accumulation** — replace the `i64 * i64 >> 16`
   per-RGB multiply in `accumulate_from_mapping` with `u32 * u32 >> 16`.
   Range analysis proves the product fits in `u32`.
2. **u8→Q32 LUT** — replace `u8_to_q32_normalized`'s `(v * 65536) / 255`
   divide with a const-evaluated 256-entry LUT. Kills the `__divdi3`
   hotspot (~2% of total cycles in the latest profile).
3. **Per-fixture channel LUT** — collapse the per-channel post-loop
   transform `Q32 → ×brightness → to_u16_saturating → (optional gamma)
   → u16` into a single 4096-entry table lookup keyed by the top 12 bits
   of the saturated accumulator. Recomputed lazily after `brightness` or
   `gamma_correction` actually changes; shed in
   `shed_optional_buffers`.

Out of scope (deliberately deferred to a follow-up plan):

- Hoisting `ctx.get_output(...)` out of the per-channel loop.
- Caching `lamp_colors` and accumulator `Vec`s on the runtime.
- Devirtualizing the `TextureSampler` trait dispatch.
- Carrying 16-bit precision through the `TextureSampler::sample_pixel`
  return type.

## File structure

```
lp-core/lp-engine/src/nodes/fixture/
├── mod.rs                   # UPDATE: pub mod channel_lut;
├── channel_lut.rs           # NEW: ChannelLut, build, lookup, reference fn, tests
├── runtime.rs               # UPDATE: hold Option<ChannelLut>, swap inner loop body,
│                            #         invalidate in update_config + shed_optional_buffers
├── gamma.rs                 # (unchanged — still the source of GAMMA8)
└── mapping/
    ├── accumulation.rs      # UPDATE: u32 mul (idea 1); LUT instead of /255 (idea 2)
    ├── entry.rs             # (unchanged)
    ├── points.rs            # (unchanged)
    ├── sampling/...         # (unchanged)
    └── overlap/...          # (unchanged)
```

No new dependencies. No public API changes. All changes are internal to
the `lp-engine` crate.

## Conceptual architecture

```
┌─────────────────────────── FixtureRuntime ──────────────────────────────┐
│                                                                          │
│  brightness: u8                channel_lut: Option<ChannelLut>           │
│  gamma_correction: bool        ↑                  ↑                     │
│                                │                  │                     │
│           update_config ───────┘                  └── shed_optional_*   │
│           (invalidates only                                              │
│            if brightness or                                              │
│            gamma changed)                                                │
└──────────────────────────────────┬───────────────────────────────────────┘
                                   │
                               render()
                                   │
                                   ▼
┌──────────────────────────── per-pixel-sample ───────────────────────────┐
│  accumulate_from_mapping(...)                                            │
│     for each entry:                                                      │
│       norm_r = U8_TO_Q32[pixel_r]      ─── idea 2: LUT, no divide       │
│       acc_r  = (norm_r as u32 * frac as u32) >> 16                       │
│                                          ─── idea 1: u32 mul, no i64    │
│       (... same for g, b ...)                                            │
│       accumulators.r[ch] += acc_r                                        │
└──────────────────────────────────┬───────────────────────────────────────┘
                                   │
                               per channel
                                   │
                                   ▼
┌──────────────────────────── per-channel post-loop ──────────────────────┐
│  let lut = self.channel_lut.get_or_insert_with(                          │
│      || ChannelLut::build(self.brightness, self.gamma_correction));     │
│                                                                          │
│  for ch in 0..=max_channel:                                              │
│      r_u16 = lut.lookup(ch_values_r[ch])                                │
│      g_u16 = lut.lookup(ch_values_g[ch])                                │
│      b_u16 = lut.lookup(ch_values_b[ch])                                │
│      lamp_colors[ch*3..ch*3+3] = [(r_u16 >> 8) as u8, ...]              │
│      ctx.get_output(...).write_rgb_u16(buffer, 0, r_u16, g_u16, b_u16)  │
│                                          ─── idea 3: LUT collapses       │
│                                              brightness × to_u16 ×      │
│                                              gamma into one load        │
└──────────────────────────────────────────────────────────────────────────┘
```

## Components

### `accumulate_from_mapping` (updated)

Inner-loop body for the partial-contribution branch becomes:

```rust
let frac    = entry.contribution_raw();             // u32 in [1, 65535]
let norm_r  = U8_TO_Q32[pixel_r as usize].0 as u32; // u32 in [0, 65535]
let norm_g  = U8_TO_Q32[pixel_g as usize].0 as u32;
let norm_b  = U8_TO_Q32[pixel_b as usize].0 as u32;

debug_assert!(frac <= 0x1_0000);
debug_assert!(norm_r <= 0xFFFF && norm_g <= 0xFFFF && norm_b <= 0xFFFF);

let acc_r = Q32(((norm_r * frac) >> 16) as i32);
let acc_g = Q32(((norm_g * frac) >> 16) as i32);
let acc_b = Q32(((norm_b * frac) >> 16) as i32);
```

`U8_TO_Q32: [Q32; 256]` is a `static` const-evaluated from
`Q32(((v as i64) * 65536 / 255) as i32)` to preserve bit-exact behaviour
with the old `u8_to_q32_normalized` function.

### `ChannelLut` (new, in `channel_lut.rs`)

```rust
pub struct ChannelLut {
    out_u16: [u16; 4096],
}

impl ChannelLut {
    pub fn build(brightness: u8, gamma: bool) -> Self {
        let mut out_u16 = [0u16; 4096];
        for bin in 0..4096u32 {
            let q = Q32(((bin << 4) as i32).min(Q32::ONE.0 - 1));
            out_u16[bin as usize] =
                channel_transform_reference(q, brightness, gamma);
        }
        Self { out_u16 }
    }

    #[inline]
    pub fn lookup(&self, ch_q32: Q32) -> u16 {
        let sat = (ch_q32.0 as u32).min(Q32::ONE.0 as u32 - 1);
        let idx = (sat >> 4) as usize;       // 0..=4095
        self.out_u16[idx]
    }
}

/// Slow-path reference: the EXACT transform the LUT collapses.
/// Used by `ChannelLut::build` and by the exhaustive sweep test.
fn channel_transform_reference(ch_q32: Q32, brightness: u8, gamma: bool) -> u16 {
    let brightness_q = brightness.to_q32() / 255.to_q32();
    let r_q = ch_q32 * brightness_q;
    let mut r = r_q.to_u16_saturating();
    if gamma {
        r = apply_gamma((r >> 8) as u8).to_q32().to_u16_saturating();
    }
    r
}
```

The LUT is built *from* the reference function, so the test is "for each
of the 4096 input bins, does `lookup(bin_q32)` reproduce the reference
output exactly?" That eliminates drift between two parallel
implementations.

### `FixtureRuntime` (updated)

- New field `channel_lut: Option<ChannelLut>`, defaulting to `None`.
- `init`: no eager build.
- `update_config`: read `new_brightness` and `new_gamma`, compare against
  the existing fields, write them, and `self.channel_lut = None` only if
  either changed.
- `shed_optional_buffers`: also `self.channel_lut = None`.
- `render`: `let lut = self.channel_lut.get_or_insert_with(|| ...);` then
  the per-channel loop becomes three `lut.lookup` calls per RGB triple,
  with `(r_u16 >> 8) as u8` for the `lamp_colors` byte.

## Validation strategy

- **Phase 1 (u32 mul):** existing accumulation tests pass; new test
  asserts bit-exact equality with the old i64 path for representative
  `(norm, frac)` pairs spanning the input range.
- **Phase 2 (u8→Q32 LUT):** new test asserts `U8_TO_Q32[v] ==
  Q32(((v as i64) * 65536 / 255) as i32)` for all 256 inputs.
- **Phase 3 (ChannelLut module):** the exhaustive sweep test described
  in `00-notes.md` Q7 — 4096 input bins × 8 brightness values × 2 gamma
  values, each asserting bit-exact equality with the reference function.
  Plus saturation and boundary tests.
- **Phase 4 (wire into FixtureRuntime):** integration test in
  `runtime.rs` tests asserting `lamp_colors` after a synthetic render
  matches a known-good vector.
- **Phase 5 (cleanup):** `cargo test -p lp-engine` and `cargo clippy
  -p lp-engine -- -D warnings`. Plus a final `just check` if the project
  has it.

Phase boundaries are committed individually so a profile run after each
commit isolates that phase's contribution.
