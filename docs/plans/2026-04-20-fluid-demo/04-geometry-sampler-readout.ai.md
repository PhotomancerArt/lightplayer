# Phase 04 — Ring geometry, sampler, readout

**Tags:** sub-agent: yes, parallel: 3, depends on phase 1.

## Scope of phase

Add three modules under `lp-fw/fw-esp32/src/tests/fluid_demo/`:

- `ring_geometry.rs` — generates the 241-lamp position table for the
  `examples/basic` fixture at startup.
- `sampler.rs` — sample (r, g, b) Q32 values from the fluid grid at
  arbitrary normalized (x, y) using nearest-neighbor *or* bilinear
  interpolation, gated by a compile-time `const SAMPLER_BILINEAR: bool`.
- `readout.rs` — convert sampled Q32 RGB triples into a packed
  `[u8; 723]` (241 lamps × 3 channels), using lp2014 "normalize-by-max"
  to preserve hue.

Update `mod.rs` to add `pub mod ring_geometry; pub mod sampler; pub mod
readout;`.

### Out of scope

- `runner.rs`, `emitters.rs`.
- Wiring `fluid_demo` into the crate root (`tests/mod.rs` is phase 5).
- Any changes to `MsaFluidSolver` (phase 1 is the only window).
- Any change to the existing `examples/basic/src/fixture.fixture/node.json` —
  this phase mirrors its parameters into Rust constants.

## Code organization reminders

- Granular file structure, one concept per file.
- Place abstract things, entry points, and tests near the **top** of
  each file.
- Place helper utility functions at the **bottom** of each file.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment so it can be found
  later.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix
  them.
- Do **not** disable, skip, or weaken existing tests to make the build
  pass.
- If something blocks completion (ambiguity, unexpected design issue),
  stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations
  from this phase plan.

## Implementation details

### `mod.rs` update

After phase 3 leaves it as `pub mod emitters;`, append:

```rust
pub mod readout;
pub mod ring_geometry;
pub mod sampler;
```

### `ring_geometry.rs`

Mirror the `RingArray` parameters from
`examples/basic/src/fixture.fixture/node.json`:

```
center: (0.5, 0.5)
diameter: 1.0   →  max_radius = 0.5
9 rings, ring k has radius = 0.5 * (k / 8) for k in 0..=8
ring_lamp_counts: [1, 8, 12, 16, 24, 32, 40, 48, 60]   (241 total)
offset_angle: 0.0
order: InnerFirst                (ring 0 first, ring 8 last)
```

```rust
//! Hardcoded `examples/basic` ring geometry — 241 lamps in 9
//! concentric rings centered at (0.5, 0.5), diameter 1.0, InnerFirst
//! order. Generated once at startup; held in a `[(f32, f32); LAMP_COUNT]`.
//!
//! Mirrors `examples/basic/src/fixture.fixture/node.json`. If that
//! fixture changes, this table must be regenerated.

use libm::{cosf, sinf};

pub const LAMP_COUNT: usize = 241;
pub const RING_LAMP_COUNTS: [u32; 9] = [1, 8, 12, 16, 24, 32, 40, 48, 60];
const CENTER: (f32, f32) = (0.5, 0.5);
const DIAMETER: f32 = 1.0;

/// Build the 241-lamp `(x, y)` table in InnerFirst order. Coordinates
/// are normalized to `[0, 1]²`. This is invoked once during
/// `runner::run` setup.
pub fn build_lamp_positions() -> [(f32, f32); LAMP_COUNT] {
    let mut out = [(0.0_f32, 0.0_f32); LAMP_COUNT];
    let mut idx = 0;
    let max_ring_index = (RING_LAMP_COUNTS.len() - 1) as f32;
    for (ring, &lamp_count) in RING_LAMP_COUNTS.iter().enumerate() {
        let ring_radius = if max_ring_index > 0.0 {
            (DIAMETER / 2.0) * (ring as f32 / max_ring_index)
        } else {
            0.0
        };
        for lamp in 0..lamp_count {
            let angle =
                (2.0 * core::f32::consts::PI * lamp as f32) / lamp_count as f32;
            let x = (CENTER.0 + ring_radius * cosf(angle)).clamp(0.0, 1.0);
            let y = (CENTER.1 + ring_radius * sinf(angle)).clamp(0.0, 1.0);
            out[idx] = (x, y);
            idx += 1;
        }
    }
    out
}
```

### `sampler.rs`

```rust
//! Sample the fluid solver's r/g/b fields at arbitrary normalized
//! (x, y) ∈ [0, 1]². Compile-time switch between nearest-neighbor and
//! bilinear interpolation.
//!
//! The solver grid has interior dimensions `nx × ny` with a 1-cell
//! ghost border; cell `(i, j)` for `1 ≤ i ≤ nx, 1 ≤ j ≤ ny` is the
//! interior. We map (x, y) into the interior. Edge cells get
//! nearest-neighbor sampled regardless of `SAMPLER_BILINEAR` to avoid
//! reaching into the ghost border.

use lps_q32::Q32;

use crate::tests::msafluid_solver::MsaFluidSolver;

/// Compile-time choice of interpolation. `false` = nearest, `true` =
/// bilinear. Bilinear costs ~4× the loads but is much smoother for the
/// circular fixture; nearest is the diagnostic baseline.
pub const SAMPLER_BILINEAR: bool = true;

/// Sample (r, g, b) from the solver at normalized (x, y) ∈ [0, 1]².
pub fn sample_rgb(solver: &MsaFluidSolver, x: f32, y: f32) -> (Q32, Q32, Q32) {
    if SAMPLER_BILINEAR {
        sample_rgb_bilinear(solver, x, y)
    } else {
        sample_rgb_nearest(solver, x, y)
    }
}

/// Nearest-neighbor sample.
pub fn sample_rgb_nearest(
    solver: &MsaFluidSolver,
    x: f32,
    y: f32,
) -> (Q32, Q32, Q32) {
    let nx = solver.nx();
    let ny = solver.ny();
    let stride = solver.stride();
    let i = ((x * nx as f32) as i32).clamp(0, nx as i32 - 1) as usize + 1;
    let j = ((y * ny as f32) as i32).clamp(0, ny as i32 - 1) as usize + 1;
    let c = i + j * stride;
    (solver.r()[c], solver.g()[c], solver.b()[c])
}

/// Bilinear sample. Falls back to nearest at the very edge to avoid
/// reading outside the interior.
pub fn sample_rgb_bilinear(
    solver: &MsaFluidSolver,
    x: f32,
    y: f32,
) -> (Q32, Q32, Q32) {
    let nx = solver.nx();
    let ny = solver.ny();
    let stride = solver.stride();

    // Continuous cell-center coordinates in interior space (1..=nx).
    let fx = x * nx as f32 + 0.5;
    let fy = y * ny as f32 + 0.5;

    let i0 = (fx.floor() as i32).clamp(1, nx as i32 - 1) as usize;
    let j0 = (fy.floor() as i32).clamp(1, ny as i32 - 1) as usize;
    let i1 = i0 + 1;
    let j1 = j0 + 1;

    let tx_f = (fx - i0 as f32).clamp(0.0, 1.0);
    let ty_f = (fy - j0 as f32).clamp(0.0, 1.0);
    let tx = Q32::from_f32_wrapping(tx_f);
    let ty = Q32::from_f32_wrapping(ty_f);
    let one = Q32::ONE;

    let c00 = i0 + j0 * stride;
    let c10 = i1 + j0 * stride;
    let c01 = i0 + j1 * stride;
    let c11 = i1 + j1 * stride;

    let lerp = |a: Q32, b: Q32, t: Q32| a + (b - a) * t;

    let r = lerp(
        lerp(solver.r()[c00], solver.r()[c10], tx),
        lerp(solver.r()[c01], solver.r()[c11], tx),
        ty,
    );
    let g = lerp(
        lerp(solver.g()[c00], solver.g()[c10], tx),
        lerp(solver.g()[c01], solver.g()[c11], tx),
        ty,
    );
    let b = lerp(
        lerp(solver.b()[c00], solver.b()[c10], tx),
        lerp(solver.b()[c01], solver.b()[c11], tx),
        ty,
    );
    let _ = one; // silence dead `one` if compiler folds it away.
    (r, g, b)
}
```

Note: `Q32::ONE` and arithmetic ops `+`, `-`, `*` are already used in
`msafluid_solver.rs`; reuse the same patterns. If `Q32::ONE` is missing,
`Q32::from_f32_wrapping(1.0)` is the fallback.

### `readout.rs`

```rust
//! Convert sampled (r, g, b) Q32 values into a packed `[u8; 723]`
//! frame buffer (241 lamps × 3 channels). lp2014 normalize-by-max
//! preserves hue: divide each channel by `max(1.0, r, g, b)` so any
//! channel above 1.0 pulls the others down proportionally.

use lps_q32::Q32;

use crate::tests::fluid_demo::ring_geometry::LAMP_COUNT;
use crate::tests::fluid_demo::sampler::sample_rgb;
use crate::tests::msafluid_solver::MsaFluidSolver;

pub const FRAME_BYTES: usize = LAMP_COUNT * 3;

/// Sample the solver at every lamp position and write the result as
/// gamma-naïve `[u8; FRAME_BYTES]` in RGB triplet order. The display
/// pipeline applies its own gamma / brightness LUTs downstream.
pub fn render_frame(
    solver: &MsaFluidSolver,
    lamp_positions: &[(f32, f32); LAMP_COUNT],
    out: &mut [u8; FRAME_BYTES],
) {
    for (i, &(x, y)) in lamp_positions.iter().enumerate() {
        let (r, g, b) = sample_rgb(solver, x, y);
        let (r8, g8, b8) = q32_rgb_to_u8_normalized(r, g, b);
        let off = i * 3;
        out[off] = r8;
        out[off + 1] = g8;
        out[off + 2] = b8;
    }
}

// ----- helpers (private) ---------------------------------------------

/// lp2014 normalize-by-max → 8-bit. Each channel is divided by
/// `max(1.0, r, g, b)`; channels below the cap pass through unchanged
/// (in normalized form), channels above the cap are scaled down
/// preserving hue. Negative values clamp to zero before normalization.
fn q32_rgb_to_u8_normalized(r: Q32, g: Q32, b: Q32) -> (u8, u8, u8) {
    let zero = Q32::ZERO;
    let one = Q32::from_f32_wrapping(1.0);
    let r = if r > zero { r } else { zero };
    let g = if g > zero { g } else { zero };
    let b = if b > zero { b } else { zero };

    let mx = max3(r, g, b);
    let denom = if mx > one { mx } else { one };

    let to_u8 = |c: Q32| -> u8 {
        let v = c / denom;
        let scaled = v * Q32::from_f32_wrapping(255.0);
        let raw = scaled.to_f32();
        let clamped = raw.clamp(0.0, 255.0);
        clamped as u8
    };

    (to_u8(r), to_u8(g), to_u8(b))
}

fn max3(a: Q32, b: Q32, c: Q32) -> Q32 {
    let ab = if a > b { a } else { b };
    if ab > c { ab } else { c }
}
```

If `Q32::to_f32` does not exist on this codebase, use whatever the
crate exposes (`as_f32`, or `Q32` raw bits → `f32` arithmetic). Search:

```sh
rg 'pub fn (to_f32|as_f32|raw)' lp-shader/lps-q32/src/q32.rs
```

If neither exists, prefer pure Q32 arithmetic: `(c / denom) * 255` then
`.raw() >> 16` (or equivalent integer extraction); the priority is
correctness, not micro-optimization. The display pipeline will smooth
over single-LSB rounding.

### Optional: a single unit test for normalize_by_max

Add at the bottom of `readout.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_passthrough_below_cap() {
        let one = Q32::from_f32_wrapping(1.0);
        let half = Q32::from_f32_wrapping(0.5);
        let zero = Q32::ZERO;
        let (r, g, b) = q32_rgb_to_u8_normalized(one, half, zero);
        assert_eq!(r, 255);
        assert!((g as i32 - 127).abs() <= 1);
        assert_eq!(b, 0);
    }

    #[test]
    fn normalize_scales_when_above_cap() {
        let two = Q32::from_f32_wrapping(2.0);
        let one = Q32::from_f32_wrapping(1.0);
        let (r, g, _) = q32_rgb_to_u8_normalized(two, one, Q32::ZERO);
        assert_eq!(r, 255);
        assert!((g as i32 - 127).abs() <= 1);
    }
}
```

Note: tests are gated by `#[cfg(test)]`, so they don't ship; they
exercise the helper on the host toolchain.

## Validate

From `lp-fw/fw-esp32/`:

```sh
cargo clippy --features esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_msafluid,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

Both must pass clean. As in phase 3, the new files compile only once
phase 5 wires `fluid_demo` into `tests/mod.rs`; that's expected and
intentional. The host-side `cargo test` is **not** run in this phase
because the crate is `no_std` esp32-only — the unit tests above will be
exercised when phase 5's full validation runs (or not at all on this
target). Leave them in place for documentation value.
