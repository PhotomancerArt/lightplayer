# Phase 6: gnoise - Smoothing Function LUTs

## Scope

Optimize gnoise gradient/value noise with LUTs for smoothing functions:
1. Quintic smoothstep LUT (gnoise3 uses quintic interpolation)
2. Cubic smoothstep LUT (gnoise2 uses cubic interpolation)
3. No-period fast path for gnoise3_tile variants

**In scope:**
- Precompute smoothstep functions at fixed-point resolution
- Replace polynomial evaluation with LUT lookup
- Split tile variants into no-tile fast path

**Out of scope:**
- Hash changes (gnoise uses different random source)
- Trig changes (gnoise doesn't use trig)

## Files to Modify

- `lp-shader/lps-builtins/src/builtins/lpfn/generative/gnoise/gnoise2_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/gnoise/gnoise3_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/gnoise/gnoise3_tile_q32.rs`

### New files
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/gnoise/smooth_lut_q32.rs`

## Implementation Details

### Smoothing Function LUT

gnoise uses smoothstep for interpolation between grid points:

**Cubic smoothstep**: `3t^2 - 2t^3` (used in 2D)
**Quintic smoothstep**: `6t^5 - 15t^4 + 10t^3` (used in 3D)

Current (computes polynomial every call):
```rust
// Cubic
let t = position.fract();
let t3 = t * t * t;  // saturating muls
let t_smooth = t * t * (Q32::THREE - Q32::TWO * t);  // 3t^2 - 2t^3

// Quintic
let t2 = t * t;
let t3 = t2 * t;
let t4 = t3 * t;
let t5 = t4 * t;
let t_smooth = t * (t * (t * (t * (t * Q32::SIX - Q32::FIFTEEN) + Q32::TEN)));
```

New (LUT-based):
```rust
// smooth_lut_q32.rs

/// Cubic smoothstep LUT: 3t^2 - 2t^3 for t in [0, 1]
/// 256 entries for t = n/256
pub const CUBIC_SMOOTHSTEP_LUT: [i32; 256] = {
    let mut lut = [0i32; 256];
    let mut i = 0;
    while i < 256 {
        let t = (i as i64) << 8;  // i/256 in Q16.16
        let t2 = (t * t) >> 16;
        let t3 = (t2 * t) >> 16;
        // 3t^2 - 2t^3 in Q16.16
        lut[i] = ((3 * t2 - 2 * t3) >> 16) as i32;
        i += 1;
    }
    lut
};

/// Quintic smoothstep LUT: 6t^5 - 15t^4 + 10t^3
pub const QUINTIC_SMOOTHSTEP_LUT: [i32; 256] = {
    // Similar pattern with quintic polynomial
};

// Usage in gnoise
let t_index = ((t.0 >> 8) & 0xFF) as usize;  // top 8 bits of fraction
let t_smooth = Q32::from_fixed(CUBIC_SMOOTHSTEP_LUT[t_index]);
```

### LUT Size

256 entries × 4 bytes = 1KB per LUT
- CUBIC_SMOOTHSTEP_LUT: 1KB
- QUINTIC_SMOOTHSTEP_LUT: 1KB
Total: 2KB rodata (acceptable)

### Resolution

256 entries gives ~0.004 precision in [0, 1], sufficient for noise quality.

### gnoise3_tile Fast Path

Current (always does mod for tile):
```rust
fn lpfn_gnoise3_tile(x: Vec3Q32, tile_length: Q32, ...) {
    // Always tile - no fast path
}
```

Check if caller uses `gnoise3` (non-tile) vs `gnoise3_tile`:
- If non-tile: no mod needed
- If tile with tile_length = 0: can skip mod

Actually, gnoise3 already has no-tile variant. The tile variant could add:
```rust
fn lpfn_gnoise3_tile(x: Vec3Q32, tile_length: Q32, ...) {
    if tile_length == Q32::ZERO {
        // Delegate to non-tile path
        return lpfn_gnoise3(x, seed);
    }
    // Full tile path
}
```

But this adds a branch at entry. Better: document that callers should use appropriate variant.

## Validate

```bash
# 1. Unit tests
cargo test -p lps-builtins gnoise

# 2. Snapshots may drift due to LUT precision
cargo test -p lps-builtins --test lpfn_q32_snapshots
# If fail: LP_UPDATE_SNAPSHOTS=1 cargo test... and document drift

# 3. Filetests
scripts/filetests.sh --target jit.q32 lp_gnoise
scripts/filetests.sh --target rv32c.q32 lp_gnoise

# 4. Profile
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p6-gnoise

# 5. Compare
# Look for reduction in gnoise self-cycles

# 6. CI gate
just check
```

## Definition of Done

- [ ] `CUBIC_SMOOTHSTEP_LUT` exists with 256 entries
- [ ] `QUINTIC_SMOOTHSTEP_LUT` exists with 256 entries
- [ ] gnoise2 uses cubic LUT for interpolation
- [ ] gnoise3 uses quintic LUT for interpolation
- [ ] Snapshots updated if drift (document: "LUT precision vs polynomial eval")
- [ ] Profile shows reduction vs p0
- [ ] `just check` clean

## Notes

LUT vs polynomial tradeoff:
- LUT: Memory load (cache hit likely), 256-step quantization
- Polynomial: ~10 saturating multiplications

On RV32 with slow multiply, LUT should win. Profile will confirm.
