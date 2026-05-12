# Phase 4: snoise2_q32 - Branchless Step and Wrapping Surflet Math

## Scope

Apply applicable psrdnoise2 patterns to snoise2:
1. Branchless simplex ordering (replace if-else with bit arithmetic)
2. Wrapping math for surflet calculations (provably bounded)

**In scope:**
- Branchless triangle order selection
- Wrapping dot products in surflet calculations
- Range analysis documentation

**Out of scope:**
- No-period fast path (snoise doesn't have period support)
- Hash changes (snoise uses different hash - noiz bit-mixing)
- Trig LUT (snoise doesn't use trig)

## Files to Modify

- `lp-shader/lps-builtins/src/builtins/lpfn/generative/snoise/snoise2_q32.rs`

## Implementation Details

### Branchless Simplex Ordering

Current (has branch mispredict):
```rust
let (order_x, order_y) = if offset1_x > offset1_y {
    // Lower triangle, XY order: (0,0)->(1,0)->(1,1)
    (Q32::ONE, Q32::ZERO)
} else {
    // Upper triangle, YX order: (0,0)->(0,1)->(1,1)
    (Q32::ZERO, Q32::ONE)
};
```

New (branchless like psrdnoise2):
```rust
// offset1_x > offset1_y ? 1 : 0
let cmp_raw = (((offset1_x.0 - offset1_y.0) >> 31).wrapping_add(1)) & 1;
let order_x = Q32((cmp_raw as i32) << 16);  // 0x10000 or 0
let order_y = Q32(((1 - cmp_raw) as i32) << 16);  // opposite
```

This eliminates the branch misprediction penalty which can be significant in tight noise loops.

### Wrapping Surflet Math

Surflet calculation involves:
1. Computing `t = 0.5 - x*x - y*y` (radial falloff threshold)
2. If `t > 0`, compute contribution: `t*t*t * dot(grad, offset)`

Bounded operations for wrapping:
```rust
// t calculation: 0.5 - x*x - y*y
// x and y are bounded by simplex geometry (~[-1, 1])
// x*x is bounded by ~1.0, so wrapping is safe
let x2 = offset_x.mul_wrapping(offset_x);  // instead of offset_x * offset_x
let y2 = offset_y.mul_wrapping(offset_y);
let t = HALF.sub_wrapping(x2.add_wrapping(y2));

// t*t*t when t > 0
// t is bounded [0, 0.5], so t*t*t is bounded
let t2 = t.mul_wrapping(t);
let t3 = t2.mul_wrapping(t);

// Dot product: grad · offset
// Both are bounded, dot product is bounded
let dot = grad_x.mul_wrapping(offset_x).add_wrapping(grad_y.mul_wrapping(offset_y));

// Final contribution
let contribution = t3.mul_wrapping(dot);
```

### Surflet Function Update

```rust
#[inline(always)]
fn surflet_2d(gi: usize, x: Q32, y: Q32) -> Q32 {
    // t = 0.5 - x^2 - y^2
    let x2 = x.mul_wrapping(x);
    let y2 = y.mul_wrapping(y);
    let t = HALF.sub_wrapping(x2.add_wrapping(y2));

    // Early return if t <= 0 (still need branch here)
    if t <= Q32::ZERO {
        return Q32::ZERO;
    }

    // t^4 * dot(grad, (x, y))  (note: t^3 * t = t^4 for weighting)
    let t2 = t.mul_wrapping(t);
    let t3 = t2.mul_wrapping(t);

    // Get gradient from GRAD_LUT_2D[gi % 8]
    let (gx, gy) = GRAD_LUT_2D[gi & 7];  // 8 gradients in 2D simplex
    let gx_q = Q32::from_fixed(gx);
    let gy_q = Q32::from_fixed(gy);

    let dot = gx_q.mul_wrapping(x).add_wrapping(gy_q.mul_wrapping(y));
    t3.mul_wrapping(dot)
}
```

### Gradient LUT for snoise

snoise uses 8 gradient directions in 2D (corners of square, not normalized):
```rust
// gradients for 2D simplex: from midpoints of cube edges
// (1,1), (-1,1), (1,-1), (-1,-1), (1,0), (-1,0), (0,1), (0,-1)
const GRAD_LUT_2D: [(i32, i32); 8] = [
    (65536, 65536),      // (1, 1)
    (-65536, 65536),     // (-1, 1)
    (65536, -65536),     // (1, -1)
    (-65536, -65536),    // (-1, -1)
    (65536, 0),          // (1, 0)
    (-65536, 0),         // (-1, 0)
    (0, 65536),          // (0, 1)
    (0, -65536),         // (0, -1)
];
```

Note: This is simpler than psrdnoise's angle-based LUT - these are fixed directions.

## Validate

```bash
# 1. Unit tests
cargo test -p lps-builtins snoise

# 2. Snapshots should pass without regeneration (wrapping only)
cargo test -p lps-builtins --test lpfn_q32_snapshots test_snoise2

# 3. Filetests
scripts/filetests.sh --target jit.q32 lp_simplex2
scripts/filetests.sh --target rv32c.q32 lp_simplex2

# 4. Profile
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p4-snoise2

# 5. Compare to p0 baseline
# Look for reduction in __lp_lpfn_snoise2_q32 self-cycles

# 6. CI gate
just check
```

## Definition of Done

- [ ] Branchless simplex ordering implemented
- [ ] Surflet calculations use wrapping math
- [ ] Range analysis comments at each wrapping site
- [ ] Snapshots pass without regeneration
- [ ] Profile shows reduction in snoise2 self-cycles vs p0
- [ ] `just check` clean

## Notes

snoise2 is widely used (fbm2 delegates to it). Even small improvements here have multiplicative impact.

Branchless step is the highest-value change - surflet wrapping is secondary but adds up across 3 corners × many calls.
