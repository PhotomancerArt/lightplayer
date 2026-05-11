# Phase 7: worley - Wrapping Distance Math

## Scope

Optimize worley cellular noise with wrapping math for distance calculations:
1. Wrapping math for distance accumulation (provably bounded)
2. (Optional) LUT for common distance metrics

**In scope:**
- Apply `mul_wrapping` / `add_wrapping` where distance calculations are bounded
- Document range analysis

**Out of scope:**
- Hash changes (worley uses noiz bit-mixing)
- Trig changes (worley doesn't use trig)
- Major algorithm changes (distance metrics are fundamental)

## Files to Modify

- `lp-shader/lps-builtins/src/builtins/lpfn/generative/worley/worley2_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/worley/worley3_q32.rs`

## Implementation Details

### Worley Algorithm Overview

Worley noise finds distance to nearest feature point in cell grid:
1. Determine which cell query point is in
2. Check neighboring cells (9 in 2D, 27 in 3D)
3. For each cell, generate random feature point using hash
4. Compute distance to each feature point
5. Return minimum distance (or other metric like F2-F1)

Current distance calculations:
```rust
// Euclidean distance (2D)
let dx = query_x - feature_x;
let dy = query_y - feature_y;
let dist_sq = dx * dx + dy * dy;  // saturating
let dist = Q32::sqrt(dist_sq);    // expensive, often skipped

// Or Manhattan distance
let dist = dx.abs() + dy.abs();
```

### Bounded Operations for Wrapping

Distance calculations are provably bounded:
- Cell size is 1.0 (Q16.16: 65536)
- Query point is within cell or neighbor (distance <= 2.0 cells)
- dx, dy bounded by ~[-2, 2]
- dx*dx bounded by ~4.0, dist_sq bounded by ~8.0

Wrapping application:
```rust
// dx, dy computation (may need saturation for raw differences)
let dx = query_x.sub_wrapping(feature_x);  // or saturating - range analysis needed
let dy = query_y.sub_wrapping(feature_y);

// Distance squared (provably bounded)
let dx2 = dx.mul_wrapping(dx);
let dy2 = dy.mul_wrapping(dy);
let dist_sq = dx2.add_wrapping(dy2);

// For Manhattan
let dist = dx.abs().add_wrapping(dy.abs());
```

**Caution**: The difference `query_x - feature_x` might overflow if both are arbitrary. But in worley:
- Query point is the input (can be any value)
- Feature point is within [cell, cell+1] of query cell
- Difference is bounded by cell neighborhood size

Range analysis:
- Input query can be any Q32 value
- Feature point is generated from cell coordinates (hash-based)
- Feature point is mapped to [0, 1] within cell
- Actual feature coordinate = cell_coord + hash_offset
- Difference = query - (cell_coord + hash_offset)
- query is in [cell_coord, cell_coord+1]
- So difference is in [-1, 1] relative to cell
- Neighbor cells add +/-1, so difference bounded by ~[-2, 2]

### Distance Metric LUT (Optional)

For common metrics, precompute:
```rust
// LUT for Euclidean distance given dx, dy in quantized units
// 64x64 table for dx, dy each in [-2, 2] with 0.0625 step
// Too large: 4096 entries × 4B = 16KB

// Smaller: 16x16 for [-2, 2] with 0.25 step
// 256 entries × 4B = 1KB - manageable

pub const EUCLIDEAN_DIST_LUT: [i32; 256] = {
    // index = (dx_quant << 4) | dy_quant
    // dx_quant: 0-15 maps to -2.0 to +2.0
};
```

Tradeoff: LUT adds quantization error to distance. Worley is often used for precise patterns - may not be worth it.

**Decision**: Skip distance LUT for now. Focus on wrapping math only.

### Wrapping Application Sites

1. **Feature point coordinate generation** (from hash):
   ```rust
   let feature_offset_x = Q32::from_fixed(hash_value).mul_wrapping(CELL_SCALE);
   ```

2. **Distance vector computation**:
   ```rust
   let dx = query_x.sub_wrapping(cell_base_x.add_wrapping(feature_offset_x));
   ```

3. **Distance squared** (confirmed safe for wrapping):
   ```rust
   let dist_sq = dx.mul_wrapping(dx).add_wrapping(dy.mul_wrapping(dy));
   ```

4. **Min distance tracking** (keep saturating - comparison needs correct ordering):
   ```rust
   if dist_sq < min_dist {  // saturating comparison
       min_dist = dist_sq;
   }
   ```

## Validate

```bash
# 1. Unit tests
cargo test -p lps-builtins worley

# 2. Filetests (lp_worley.glsl created in Phase 1)
scripts/filetests.sh --target jit.q32 lp_worley
scripts/filetests.sh --target rv32c.q32 lp_worley

# 3. Snapshots should pass
cargo test -p lps-builtins --test lpfn_q32_snapshots test_worley

# 4. Profile
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p7-worley

# 5. Compare
# Expect modest improvement (worley not used in fastmath baseline,
# but profile should still show reduction if tested)

# 6. CI gate
just check
```

## Definition of Done

- [ ] Range analysis comments at each wrapping site
- [ ] Distance squared uses wrapping math
- [ ] Feature offset calculation uses wrapping math
- [ ] Min-tracking comparisons remain saturating (correctness critical)
- [ ] Filetests pass
- [ ] Snapshots pass without regeneration
- [ ] `just check` clean

## Notes

Worley has less optimization potential than psrdnoise/snoise:
- No trig to eliminate
- No period handling to optimize
- Hash is already efficient (noiz bit-mixing)

Wrapping math is the main win - modest but measurable.
