# Phase 3: psrdnoise3_q32 - Wrapping Math and No-Period Fast Path

## Scope

Apply remaining psrdnoise2 optimizations:
1. Wrapping Q32 math where provably bounded
2. No-period fast path (skip mod289 when period=0)

**In scope:**
- Add `mul_wrapping` / `add_wrapping` calls in hot path
- Create `psrdnoise3_noperiod()` helper
- Split main function to dispatch to fast path

**Out of scope:**
- Hash changes (done in Phase 2)
- Trig changes (done in Phase 2)

## Files to Modify

- `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/psrdnoise3_q32.rs`

## Implementation Details

### Wrapping Math Identification

From psrdnoise2 analysis, these operations are provably bounded:

1. **Dot products** (`x_k.dot(grad_k)`):
   - `x_k` components bounded by simplex geometry (~[-1, 1] in skewed space)
   - `grad_k` components are normalized (sin/cos outputs, sphere points)
   - Dot product bounded by ~4.0 max → safe for Q16.16 wrapping

2. **Radial decay polynomial** (`w*w*w*w`):
   - `w` is falloff weight, in [0, 1] range
   - `w*w*w*w` bounded → wrapping safe

3. **Gradient accumulation**:
   - Accumulating 4 corner contributions (tetrahedron has 4 corners)
   - Each contribution bounded → sum bounded

Implementation:
```rust
// Before
let t = x0.dot(g0);
let w = Q32::ONE - RADIAL_DECAY_0_5 * dist2;  // saturating

// After
let t = x0.dot_wrapping(g0);  // use mul_wrapping inside dot
let w = Q32::ONE.sub_wrapping(RADIAL_DECAY_0_5.mul_wrapping(dist2));
```

### No-Period Fast Path

Current (always does period handling):
```rust
pub fn lpfn_psrdnoise3(x: Vec3Q32, period: Vec3Q32, ...) {
    // ... simplex transform ...

    // Always compute wrapped coordinates
    let wrapped = if period.x > 0 || period.y > 0 || period.z > 0 {
        // expensive modulo operations
    } else {
        // just copy, but still in the branch
    };
}
```

New (split like psrdnoise2):
```rust
#[inline(always)]
fn psrdnoise3_noperiod(x: Vec3Q32, alpha: Q32, seed: u32) -> (Q32, Q32, Q32, Q32) {
    // Same as main but without any period-related code
    // Skip all modulo operations, use raw cell indices
}

pub fn lpfn_psrdnoise3(x: Vec3Q32, period: Vec3Q32, alpha: Q32, seed: u32) -> (Q32, Q32, Q32, Q32) {
    if period.x == Q32::ZERO && period.y == Q32::ZERO && period.z == Q32::ZERO {
        return psrdnoise3_noperiod(x, alpha, seed);
    }
    // Full version with period handling
}
```

### Q32 Helper Verification

Ensure `lps-q32` crate exports:
- `mul_wrapping()` - for bounded multiplications
- `add_wrapping()` / `sub_wrapping()` - for bounded accumulations
- `Vec3Q32::dot_wrapping()` - helper using mul_wrapping internally

If not present, add to `lp-shader/lps-q32/src/q32.rs`:
```rust
impl Q32 {
    pub fn mul_wrapping(self, rhs: Q32) -> Q32 {
        Q32(((self.0 as i64 * rhs.0 as i64) >> 16) as i32)
    }
    // similar for add/sub
}

impl Vec3Q32 {
    pub fn dot_wrapping(self, other: Vec3Q32) -> Q32 {
        self.x.mul_wrapping(other.x)
            .add_wrapping(self.y.mul_wrapping(other.y))
            .add_wrapping(self.z.mul_wrapping(other.z))
    }
}
```

## Validate

```bash
# 1. Unit tests pass (no output drift expected for wrapping-only changes)
cargo test -p lps-builtins psrdnoise3

# 2. Snapshots should pass WITHOUT regeneration (if wrapping is correct)
cargo test -p lps-builtins --test lpfn_q32_snapshots test_psrdnoise3
# If snapshots fail, wrapping math is wrong - fix before proceeding

# 3. Filetests
scripts/filetests.sh --target jit.q32 lp_psrdnoise
scripts/filetests.sh --target rv32c.q32 lp_psrdnoise

# 4. Profile
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p3-psrdnoise3-wrapping

# 5. Compare to p2 and p0
# Expected: further reduction in psrdnoise3 self-cycles

# 6. CI gate
just check
```

## Definition of Done

- [ ] `psrdnoise3_noperiod()` helper exists with no mod operations
- [ ] Main function dispatches to fast path when period is all-zero
- [ ] Dot products use `mul_wrapping` internally
- [ ] Radial decay uses `mul_wrapping` for polynomial
- [ ] Gradient accumulation uses wrapping math
- [ ] Snapshots pass without regeneration (proving wrapping math correctness)
- [ ] Profile shows further reduction vs p2-psrdnoise3-hash-lut
- [ ] `just check` clean

## Safety Notes

Wrapping math is only safe where mathematically provable:
- Document each `mul_wrapping` site with range analysis comment
- If any snapshot shows change, either:
  a) Range analysis was wrong (bug), OR
  b) Saturating was masking an overflow that should have happened

In both cases, escalate to main agent before proceeding.
