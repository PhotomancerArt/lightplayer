# Phase 2: psrdnoise3_q32 - Integer Hash Chain and Trig LUT

## Scope

Apply the two biggest wins from psrdnoise2 to psrdnoise3:
1. Integer hash chain (replace Q32 mod289 + permute with i32 rem_euclid)
2. Gradient angle LUT (replace Fibonacci spiral sin/cos with hash-indexed LUT + alpha rotation)

**In scope:**
- Refactor hash computation to use i32-only arithmetic
- Create gradient angle LUT for 3D (different from 2D due to Fibonacci spiral)
- Replace per-corner sin/cos calls with LUT lookups
- Update snapshots (output WILL drift - document why)

**Out of scope:**
- Wrapping math (Phase 3)
- No-period fast path (Phase 3)

## Files to Modify

- `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/psrdnoise3_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/psrdnoise3_f32.rs` (verify still delegates)

### New files
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/fibonacci_lut_q32.rs` - Fibonacci spiral LUT
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/fibonacci_lut_q32_data.rs` - Generated data

## Implementation Details

### Integer Hash Chain

Current (uses saturated Q32 mul in permute):
```rust
fn mod289_q32(x: i32) -> i32 { __lps_mod_q32(x, PERIOD_289.to_fixed()) }
fn permute_q32(v: i32) -> i32 {
    let v_q32 = Q32::from_fixed(v);
    let temp = v_q32 * HASH_CONST_34 + Q32::ONE;
    mod289_q32((temp * v_q32).to_fixed())  // SATURATING MUL HERE
}
```

New (i32-only, like psrdnoise2):
```rust
#[inline(always)]
fn hash_corner(iu: i32, iv: i32, iw: i32) -> i32 {
    // 3D permutation: permute(permute(permute(iw) + iv) + iu)
    let h = iw.rem_euclid(289);
    let h = ((h * 34 + 1) * h).rem_euclid(289);
    let h = ((h + iv).rem_euclid(289) * 34 + 1) * h;
    let h = (h + iu).rem_euclid(289);
    ((h * 34 + 10) * h).rem_euclid(289)
}
```

### Fibonacci Spiral LUT

In psrdnoise3, gradients use Fibonacci spiral on sphere:
```rust
// Current: compute per corner
let theta = Q32::from_fixed(hash_x.wrapping_mul(THETA_MULT.0)); // hash * 3.883...
let sz = Q32::from_fixed(hash_x.wrapping_mul(SZ_MULT.0)) + SZ_ADD; // hash * -0.0069 + 0.9965
let (sin_t, cos_t) = fns::sin_cos_q32(theta);
let gx = cos_t * sz;
let gy = sin_t * sz;
let gz = sz; // Actually more complex...
```

New approach: Precompute 289 (cos θ, sin θ, sz) tuples:
```rust
// fibonacci_lut_q32.rs
pub struct FibonacciEntry {
    pub cos_theta: i32,
    pub sin_theta: i32,
    pub sz: i32,  // z coordinate on unit sphere
}

pub const FIBONACCI_LUT: [FibonacciEntry; 289] = [
    // Generated at compile time
];

// Usage: lookup + rotate by alpha
let entry = &FIBONACCI_LUT[hash_x as usize];
// Rotate (cos_t, sin_t) by alpha
let (sin_a, cos_a) = sincos_lut.lookup(alpha_bucket); // or compute once
let gx = ((entry.cos_theta as i64 * cos_a as i64 - entry.sin_theta as i64 * sin_a as i64) >> 16) as i32;
let gy = ((entry.sin_theta as i64 * cos_a as i64 + entry.cos_theta as i64 * sin_a as i64) >> 16) as i32;
```

### Alpha Rotation LUT

Since alpha varies per call (not per hash), compute `(sin α, cos α)` once:
```rust
let (sin_alpha, cos_alpha) = __lps_sincos_q32(alpha.to_fixed());
// Store as Q32 for reuse at each corner
```

### Integration Points

Update `psrdnoise3` tail function:
```rust
fn psrdnoise3_tail(
    x0: Vec3Q32, x1: Vec3Q32, x2: Vec3Q32, x3: Vec3Q32,
    corner_indices: CornerIndices3D,
    alpha: Q32,
) -> (Q32, Q32, Q32, Q32) {
    // Hash corners with i32-only math
    let hash_0 = hash_corner(iu_0, iv_0, iw_0);
    let hash_1 = hash_corner(iu_1, iv_1, iw_1);
    let hash_2 = hash_corner(iu_2, iv_2, iw_2);
    let hash_3 = hash_corner(iu_3, iv_3, iw_3);

    // Compute alpha rotation once
    let (sin_alpha, cos_alpha) = __lps_sincos_q32(alpha.to_fixed());
    let sin_a = Q32::from_fixed(sin_alpha);
    let cos_a = Q32::from_fixed(cos_alpha);

    // LUT lookups + rotation for each corner
    let (gx_0, gy_0, gz_0) = grad_from_hash(hash_0, sin_a, cos_a);
    // ... etc
}
```

## Validate

```bash
# 1. Unit tests still pass (range checks)
cargo test -p lps-builtins psrdnoise3

# 2. Snapshots will fail - regenerate with documentation
cd /Users/yona/dev/photomancer/feature/lightplayer-emu-perf-psrdnoise2-q32
LP_UPDATE_SNAPSHOTS=1 cargo test -p lps-builtins --test lpfn_q32_snapshots test_psrdnoise3

# 3. Verify new snapshots pass
cargo test -p lps-builtins --test lpfn_q32_snapshots test_psrdnoise3

# 4. Filetests (tolerance-based, may need re-bless)
scripts/filetests.sh --target jit.q32 lp_psrdnoise
scripts/filetests.sh --target rv32c.q32 lp_psrdnoise

# 5. Profile
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p2-psrdnoise3-hash-lut

# 6. Compare
# Look for reduction in __lp_lpfn_psrdnoise3_q32 self-cycles
# Look for reduction in __lps_sin_q32 / __lps_cos_q32 calls (replaced by LUT)

# 7. CI gate
just check
```

## Definition of Done

- [ ] `hash_corner()` uses i32-only `rem_euclid(289)` chain
- [ ] `FIBONACCI_LUT` exists with 289 precomputed entries
- [ ] Per-corner trig calls replaced with LUT + alpha rotation
- [ ] Snapshots regenerated and committed with explanation in commit body
- [ ] Filetests pass (re-bless if tolerances need adjustment - escalate if >0.001 drift)
- [ ] Profile shows reduction in psrdnoise3 self-cycles vs p0-baseline
- [ ] `just check` clean

## Notes on Output Drift

The integer hash chain produces **different** (but more correct) outputs:
- Old: Saturated Q32 silently overflows on `(hash * 34 + 1) * hash` when hash > ~7000
- New: i32 properly handles full [0, 288] range

Document in commit: "Output drift expected - integer hash eliminates saturated overflow, bringing results closer to GLSL/Lygia reference."
