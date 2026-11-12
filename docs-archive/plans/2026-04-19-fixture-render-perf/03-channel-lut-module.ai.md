# Phase 03 — ChannelLut module (new file, no integration)

**Sub-agent:** yes (Composer 2)
**Parallel:** —
**Profile after:** no (no runtime behaviour change yet — saves a
profile run)

## Scope of phase

Add a new file `lp-core/lp-engine/src/nodes/fixture/channel_lut.rs`
containing the `ChannelLut` struct, its `build` and `lookup` methods, a
slow-path `channel_transform_reference` function, and an exhaustive
sweep test. Wire it into the module tree via `mod.rs`.

This phase does **not** integrate `ChannelLut` into `FixtureRuntime` —
that's phase 04. The split exists so phase 04's diff is small and
focused on plumbing, while this phase carries the meaty algorithm and
its tests.

**Out of scope:**

- Any changes to `runtime.rs` (phase 04).
- Any changes to `accumulation.rs` (phases 01/02).
- Anything in `gamma.rs`.
- A profile run (no runtime change to attribute to).

## Code organization reminders

- One concept per file: `ChannelLut` is its own module — `channel_lut.rs`.
- Sibling files in `nodes/fixture/` follow the same one-concept pattern
  (`gamma.rs`, `runtime.rs`, etc.). Match their layout.
- File order, top to bottom: `mod tests`, then `pub struct`, then
  `impl`, then private helpers (the reference function).
- Tests at the top via `#[cfg(test)] mod tests`. Helpers (test-local)
  at the bottom of the test module.
- No temporary code; no `TODO`s.

## Sub-agent reminders

- Do not commit until the main agent has reviewed the diff.
- Do not modify `runtime.rs` — wiring is phase 04's job.
- Do not change `apply_gamma`, `GAMMA8`, or anything in `gamma.rs`.
- Do not weaken the exhaustive sweep test.
- Do not introduce `unsafe { get_unchecked }` for the LUT lookup; an
  index-bounded `[u16; 4096]` lookup is fine and LLVM usually elides
  the check.
- If the exhaustive test fails for any input bin: **stop and report**.
  That means either the reference function or the build loop is wrong;
  do not adjust the reference to match the LUT.
- Report back: files changed, validation output, deviations.

## Implementation details

### File 1: `lp-core/lp-engine/src/nodes/fixture/channel_lut.rs` (new)

```rust
//! Per-fixture channel transform lookup table.
//!
//! Collapses the per-channel post-loop transform
//! `Q32 → ×brightness → to_u16_saturating → (optional gamma) → u16`
//! into a single 4096-entry lookup keyed by the top 12 bits of the
//! saturated accumulator. Rebuilt by `FixtureRuntime` whenever
//! `brightness` or `gamma_correction` changes.

use lps_q32::q32::{Q32, ToQ32};

use super::gamma::apply_gamma;

const BIN_COUNT: usize = 4096;

/// 12-bit-input lookup table for the per-channel post-loop transform.
///
/// Memory cost: 4096 * 2 bytes = 8 KB per fixture. Sheddable by
/// `FixtureRuntime::shed_optional_buffers`.
pub struct ChannelLut {
    out_u16: [u16; BIN_COUNT],
}

impl ChannelLut {
    /// Build a fresh LUT for the given brightness/gamma combination.
    ///
    /// Each bin's u16 output is computed by `channel_transform_reference`,
    /// so the LUT is bit-exact with the reference by construction.
    pub fn build(brightness: u8, gamma: bool) -> Self {
        let mut out_u16 = [0u16; BIN_COUNT];
        for bin in 0..BIN_COUNT {
            let q = bin_to_q32(bin);
            out_u16[bin] = channel_transform_reference(q, brightness, gamma);
        }
        Self { out_u16 }
    }

    /// Look up the post-loop transform for a Q32 channel value.
    ///
    /// Saturates inputs at or above `Q32::ONE` to the same bin as
    /// `Q32::ONE - 1` (mirroring `to_u16_saturating`'s saturation).
    #[inline]
    pub fn lookup(&self, ch_q32: Q32) -> u16 {
        // ch_q32.0 may be negative or >= ONE; saturate to [0, ONE - 1].
        let raw = ch_q32.0;
        let sat: u32 = if raw < 0 {
            0
        } else {
            (raw as u32).min(Q32::ONE.0 as u32 - 1)
        };
        let idx = (sat >> 4) as usize; // 0..=4095
        self.out_u16[idx]
    }
}

/// Map a 12-bit bin index to the Q32 value at that bin's lower edge.
#[inline]
fn bin_to_q32(bin: usize) -> Q32 {
    // bin is 0..4096, so (bin << 4) is 0..65536. Clamp to ONE - 1 so
    // bin=4095 → Q32(65520), and the reference function never sees a
    // value at or above ONE here. Saturation is handled in `lookup`.
    let raw = ((bin as i32) << 4).min(Q32::ONE.0 - 1);
    Q32(raw)
}

/// Slow-path reference: the EXACT transform that the LUT collapses.
/// Used by `ChannelLut::build` (single source of truth) and by the
/// exhaustive sweep test.
fn channel_transform_reference(ch_q32: Q32, brightness: u8, gamma: bool) -> u16 {
    let brightness_q = brightness.to_q32() / 255.to_q32();
    let r_q = ch_q32 * brightness_q;
    let mut r = r_q.to_u16_saturating();
    if gamma {
        r = apply_gamma((r >> 8) as u8).to_q32().to_u16_saturating();
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_matches_reference_exhaustive() {
        for &brightness in &[0u8, 1, 8, 32, 64, 127, 200, 255] {
            for &gamma in &[false, true] {
                let lut = ChannelLut::build(brightness, gamma);
                for bin in 0..BIN_COUNT {
                    let q = bin_to_q32(bin);
                    let expected = channel_transform_reference(q, brightness, gamma);
                    assert_eq!(
                        lut.out_u16[bin], expected,
                        "bin={bin} brightness={brightness} gamma={gamma}"
                    );
                }
            }
        }
    }

    #[test]
    fn lookup_saturates_above_one() {
        let lut = ChannelLut::build(255, false);
        let last_bin = lut.out_u16[BIN_COUNT - 1];
        // Inputs at or above ONE collapse to the same bin as ONE - 1.
        assert_eq!(lut.lookup(Q32::ONE), last_bin);
        assert_eq!(lut.lookup(Q32(Q32::ONE.0 + 1)), last_bin);
        assert_eq!(lut.lookup(Q32(i32::MAX)), last_bin);
    }

    #[test]
    fn lookup_saturates_below_zero() {
        let lut = ChannelLut::build(255, false);
        assert_eq!(lut.lookup(Q32(-1)), lut.out_u16[0]);
        assert_eq!(lut.lookup(Q32(i32::MIN)), lut.out_u16[0]);
    }

    #[test]
    fn brightness_zero_yields_all_zeros() {
        for &gamma in &[false, true] {
            let lut = ChannelLut::build(0, gamma);
            for (bin, &v) in lut.out_u16.iter().enumerate() {
                assert_eq!(v, 0, "non-zero output at bin={bin} gamma={gamma}");
            }
        }
    }

    #[test]
    fn lookup_matches_reference_for_arbitrary_inputs() {
        let lut = ChannelLut::build(64, true);
        for &raw in &[0i32, 1, 1024, 16_384, 32_768, 49_152, 65_535, 65_519] {
            let q = Q32(raw);
            let from_lut = lut.lookup(q);
            let from_ref = channel_transform_reference(
                Q32(raw.min(Q32::ONE.0 - 1)),
                64,
                true,
            );
            assert_eq!(from_lut, from_ref, "raw={raw}");
        }
    }
}
```

### File 2: `lp-core/lp-engine/src/nodes/fixture/mod.rs` (edit)

Add a single line to expose the new module. The exact insertion point
depends on the existing module layout — keep it grouped with the other
`pub mod` declarations:

```rust
pub mod channel_lut;
```

If sibling modules are declared `pub(crate) mod`, match that
visibility. Otherwise `pub mod` is fine — `ChannelLut` will be used
from `runtime.rs` in the same crate either way.

### Conventions to match

- Imports at the top, grouped: `core` / `alloc` / external / `super`.
- `Q32::ZERO`, `Q32::ONE` are `pub const`s defined in `lps-q32`
  (`lp-shader/lps-q32/src/q32.rs:39`).
- `ToQ32` is a trait with `to_q32(self) -> Q32` impls for `i32`, `i16`,
  `i8`, `u16`, `u8`. The reference function uses the `u8` impl
  (`brightness.to_q32()`) and the `i32` impl (`255.to_q32()`).
- `apply_gamma` lives in `super::gamma` and takes `u8 -> u8`.

## Validate

```bash
cargo test -p lp-engine --lib nodes::fixture::channel_lut
cargo test -p lp-engine --lib nodes::fixture
```

If green:

```bash
cargo test -p lp-engine
cargo clippy -p lp-engine -- -D warnings 2>&1 | rg -A2 channel_lut
```

The exhaustive sweep is `4096 * 8 * 2 = 65_536` assertions. It should
finish in well under a second; if it's measurably slower than that,
that's worth flagging.

## Commit

After main-agent review:

```bash
git add lp-core/lp-engine/src/nodes/fixture/channel_lut.rs \
        lp-core/lp-engine/src/nodes/fixture/mod.rs
git commit -m "$(cat <<'EOF'
feat(lp-engine): add ChannelLut module for fixture render transform

- New nodes/fixture/channel_lut.rs containing ChannelLut, its build
  and lookup methods, and the channel_transform_reference function
  that ChannelLut::build is constructed from (single source of truth).
- 12-bit input (4096 bins) → u16 output. 8 KB per built table. Will
  be plugged into FixtureRuntime in the next phase.
- Exhaustive sweep test: 4096 bins × 8 brightness × 2 gamma values,
  asserting bit-exact equality with the reference function.
- Saturation, sign-extension, and brightness=0 boundary tests.

No runtime behaviour change yet — this phase only adds the module.
Phase 04 wires it into FixtureRuntime.

Plan: docs/plans/2026-04-19-fixture-render-perf/03-channel-lut-module.md
EOF
)"
```

## No profile capture this phase

`FixtureRuntime` is unchanged, so a profile would be identical to
phase 02's. Move directly to phase 04.
