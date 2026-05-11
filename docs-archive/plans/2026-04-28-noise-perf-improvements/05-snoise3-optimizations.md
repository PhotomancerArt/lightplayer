# Phase 5: snoise3_q32 - Apply snoise2 Patterns

## Scope

Apply same optimizations from snoise2 to snoise3:
1. Branchless tetrahedron ordering
2. Wrapping surflet math

**In scope:**
- Branchless 3D simplex ordering (more complex than 2D)
- Wrapping math for 3D surflets

**Out of scope:**
- Hash changes (snoise uses noiz bit-mixing, not 289)
- Trig changes (snoise doesn't use trig)
- New filetests (snoise3 already covered by lp_simplex3.glsl)

## Files to Modify

- `lp-shader/lps-builtins/src/builtins/lpfn/generative/snoise/snoise3_q32.rs`

## Implementation Details

### 3D Branchless Simplex Ordering

3D simplex has a tetrahedron with 4 corners. Determining which simplex is more complex than 2D:

Current approach uses ranking of x, y, z components:
```rust
// f0 = fract(skewed)
// Determine ordering by comparing components
if f0.x >= f0.y && f0.y >= f0.z {
    // x >= y >= z ordering
} else if f0.x >= f0.z && f0.z >= f0.y {
    // x >= z >= y ordering
}
// ... 6 total orderings for 3D
```

Branchless approach using bit manipulation:
```rust
// Compute comparison flags as bits
let xy = ((f0.x.0 - f0.y.0) >> 31) & 1;  // 1 if x < y, else 0
let xz = ((f0.x.0 - f0.z.0) >> 31) & 1;
let yz = ((f0.y.0 - f0.z.0) >> 31) & 1;

// Build ordering index (0-5 from comparisons)
let order_idx = (xy << 2) | (xz << 1) | yz;  // simplified - need actual mapping

// Use lookup table for offsets based on order_idx
const SIMPLEX_OFFSETS: [[(i32, i32, i32); 3]; 6] = [
    // 6 orderings × 3 middle corner offsets
];
```

However, this may be more complex than beneficial. **Alternative**: Only apply wrapping math, leave branching for 3D (more branches, harder to make branchless).

**Recommendation**: Phase 5 focuses on wrapping surflet math only. If 3D branchless proves valuable, make it a follow-up phase.

### 3D Wrapping Surflet Math

3D surflets have more operations:
```rust
// t = 0.6 - x^2 - y^2 - z^2  (0.6 is 3D threshold)
let x2 = x.mul_wrapping(x);
let y2 = y.mul_wrapping(y);
let z2 = z.mul_wrapping(z);
let t = THRESHOLD_3D.sub_wrapping(x2.add_wrapping(y2).add_wrapping(z2));

// t^4 (actually implemented as t^2 * t^2 for efficiency)
let t2 = t.mul_wrapping(t);
let t4 = t2.mul_wrapping(t2);

// Dot product with 3D gradient
let dot = gx.mul_wrapping(x)
    .add_wrapping(gy.mul_wrapping(y))
    .add_wrapping(gz.mul_wrapping(z));

// Contribution
let contribution = t4.mul_wrapping(dot);
```

3D gradients are from cube edge midpoints (12 directions):
```rust
const GRAD_LUT_3D: [(i32, i32, i32); 12] = [
    (1, 1, 0), (1, -1, 0), (-1, 1, 0), (-1, -1, 0),
    (1, 0, 1), (1, 0, -1), (-1, 0, 1), (-1, 0, -1),
    (0, 1, 1), (0, 1, -1), (0, -1, 1), (0, -1, -1),
].map(|(x, y, z)| (x * 65536, y * 65536, z * 65536));
```

## Validate

```bash
# 1. Unit tests
cargo test -p lps-builtins snoise3

# 2. Snapshots should pass
cargo test -p lps-builtins --test lpfn_q32_snapshots test_snoise3

# 3. Filetests
scripts/filetests.sh --target jit.q32 lp_simplex3
scripts/filetests.sh --target rv32c.q32 lp_simplex3

# 4. Profile
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p5-snoise3

# 5. Compare
# Look for reduction in __lp_lpfn_snoise3_q32 self-cycles

# 6. CI gate
just check
```

## Definition of Done

- [ ] 3D surflet calculations use wrapping math
- [ ] Range analysis comments at each wrapping site
- [ ] (Optional) Branchless ordering if complexity justifies
- [ ] Snapshots pass without regeneration
- [ ] Profile shows reduction vs p0
- [ ] `just check` clean

## Notes

3D branchless is harder than 2D. If initial attempt is complex, document and skip - wrapping math is the reliable win here.
