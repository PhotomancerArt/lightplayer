# Noise Function Performance Improvements - Design

## Scope of Work

Systematically apply proven optimization patterns from `psrdnoise2_q32` to all noise functions in `lps-builtins`:

1. **Integer hash chains** (where 289-based hash is used)
2. **Trig LUTs** (where gradient angles are hash-derived)
3. **Branchless step** (where simplex ordering is determined)
4. **Wrapping Q32 math** (where operations are provably bounded)
5. **No-period fast paths** (where period=0 can skip mod operations)
6. **Combined sincos** (where sin+cos evaluated at same angle)

## File Structure

```
lp-shader/lps-builtins/src/builtins/lpfn/generative/
├── psrdnoise/
│   ├── psrdnoise2_q32.rs          # BASELINE (already optimized)
│   ├── psrdnoise3_q32.rs          # UPDATE: Integer hash, trig LUT, wrapping math
│   ├── psrdnoise3_f32.rs          # (no change - delegates to q32)
│   └── grad_lut_q32.rs            # EXISTS (reuse pattern)
├── snoise/
│   ├── snoise2_q32.rs             # UPDATE: Branchless step, wrapping surflet math
│   ├── snoise3_q32.rs             # UPDATE: Same patterns as snoise2
│   ├── snoise2_f32.rs             # (no change)
│   └── grad_lut_q32.rs            # NEW: Gradient direction LUT (256 entries)
├── gnoise/
│   ├── gnoise2_q32.rs             # UPDATE: Smoothing function LUT
│   ├── gnoise3_q32.rs             # UPDATE: Smoothing function LUT
│   ├── gnoise3_tile_q32.rs        # UPDATE: noperiod fast path
│   └── smooth_lut_q32.rs          # NEW: Quintic/cubic smoothstep LUT
├── worley/
│   ├── worley2_q32.rs             # UPDATE: Wrapping distance math
│   ├── worley3_q32.rs             # UPDATE: Wrapping distance math
│   └── (no LUT - distance-based not gradient-based)
├── random/
│   ├── random2_q32.rs             # UPDATE: Combined sincos optimization
│   └── random3_q32.rs             # UPDATE: Combined sincos optimization
└── srandom/
    └── srandom*_q32.rs            # UPDATE: Combined sincos optimization

lp-shader/lps-builtins/tests/
├── lpfn_q32_snapshots.rs          # UPDATE: Add all noise functions
└── snapshots/lpfn_q32/
    ├── psrdnoise2_q32.snap.txt    # EXISTS
    ├── psrdnoise3_q32.snap.txt    # UPDATE (from drift)
    ├── snoise2_q32.snap.txt       # NEW
    ├── snoise3_q32.snap.txt       # NEW
    ├── gnoise2_q32.snap.txt       # NEW
    ├── gnoise3_q32.snap.txt       # NEW
    ├── worley2_q32.snap.txt       # NEW
    ├── worley3_q32.snap.txt       # NEW
    ├── random2_q32.snap.txt       # NEW
    ├── random3_q32.snap.txt       # NEW
    └── srandom*_q32.snap.txt       # NEW

lp-shader/lps-filetests/filetests/lpfn/
├── lp_psrdnoise.glsl              # EXISTS
├── lp_simplex2.glsl               # EXISTS
├── lp_simplex3.glsl               # EXISTS
├── lp_simplex1.glsl               # EXISTS
├── lp_gnoise.glsl                 # EXISTS
├── lp_fbm.glsl                    # EXISTS
├── lp_worley.glsl                 # NEW (create before optimizing)
└── lp_srandom.glsl                # NEW (create before optimizing)
```

## Conceptual Architecture

### Optimization Pattern Hierarchy

```
┌─────────────────────────────────────────────────────────────────┐
│                  Performance Patterns                            │
├─────────────────────────────────────────────────────────────────┤
│ 1. Integer Hash Chain (psrdnoise family only)                   │
│    - rem_euclid(289) i32 math vs saturated Q32 mul            │
│    - 9 mod calls → 3 i32 hash_corner calls                    │
├─────────────────────────────────────────────────────────────────┤
│ 2. Gradient Angle LUT (psrdnoise family only)                   │
│    - 289 entries × (cos, sin) × 4B = 2312B rodata             │
│    - 6 Taylor series evals → 3 LUT lookups + rotation         │
├─────────────────────────────────────────────────────────────────┤
│ 3. Branchless Step (snoise family)                              │
│    - (((x.0 - y.0) >> 31).wrapping_add(1)) << 16               │
│    - Removes branch mispredict in simplex ordering              │
├─────────────────────────────────────────────────────────────────┤
│ 4. Wrapping Q32 Math (all functions)                            │
│    - mul_wrapping() for bounded multiplications                 │
│    - Removes saturation checks in hot loops                     │
├─────────────────────────────────────────────────────────────────┤
│ 5. No-Period Fast Path (psrdnoise, gnoise tile)               │
│    - Split into _noperiod() helper                              │
│    - Skip __lps_mod_q32 calls when period=0                     │
├─────────────────────────────────────────────────────────────────┤
│ 6. Combined Sincos (random/srandom family)                      │
│    - __lps_sincos_q32() vs separate sin+cos calls              │
│    - Share range reduction, temp_angle_sq                       │
└─────────────────────────────────────────────────────────────────┘
```

### Validation Flow

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌──────────┐
│   Modify    │───▶│  Regenerate  │───▶│   Filetests  │───▶│ Profile  │
│   Code      │    │  Snapshots   │    │   Pass      │    │ Check    │
└─────────────┘    └──────────────┘    └─────────────┘    └──────────┘
       │                                     │
       │         ┌───────────────────────────┘
       │         ▼
       │    ┌─────────────┐
       └───▶│   CI Gate   │
            │ just check  │
            │ test        │
            └─────────────┘
```

## Main Components

### 1. Shared Q32 Helpers (lp-shader/lps-q32/src/q32.rs)

Ensure available for all optimizations:
- `half()` - shift-right 1 (vs * HALF)
- `mul_wrapping()` - i64→i32 narrowing without saturation
- `add_wrapping()` / `sub_wrapping()` - unchecked arithmetic

### 2. LUT Generators

Pattern from `grad_lut_q32.rs`:
```rust
const GRAD_COS_SIN_LUT: [(i32, i32); 289] = [
    // Generated at compile time from hash * 0.07482
];
```

### 3. Snapshot Test Framework

Pattern from `lpfn_q32_snapshots.rs`:
```rust
fn assert_snapshot(name: &str, actual: &str) {
    // LP_UPDATE_SNAPSHOTS=1 to regenerate
    // Otherwise compare with include_str!
}
```

### 4. Profile Comparison

Each phase produces `profiles/<ts>--examples-perf-fastmath--steady-render--p<N>-*/`
Final phase compares baseline vs final for:
- `__lp_lpfn_psrdnoise3_q32` self-cycles
- `__lp_lpfn_snoise2_q32` self-cycles
- `__lp_lpfn_snoise3_q32` self-cycles
- `__lps_sin_q32` / `__lps_cos_q32` / `__lps_sincos_q32` (should decrease)

## Decisions for Future Reference

### Integer Hash vs Saturated Q32

**Decision**: Use i32 `rem_euclid(289)` for hash chains where values are provably in [0, 288].

**Why**: Saturated Q32 silently overflows at ~2^31, causing hash collisions and drift from reference.

**Revisit when**: If hash range exceeds i32 bounds (289 is safely within).

### Gradient LUT Size

**Decision**: 289 entries for psrdnoise family (matches permute period).

**Why**: Algorithm-specific; 289 is the permutation modulus in Lygia reference.

**Revisit when**: Different noise algorithm with different permutation period.

### Wrapping Math Preconditions

**Decision**: Only use `mul_wrapping` where inputs are provably bounded (simplex geometry, etc.).

**Why**: Unbounded wrapping causes visible artifacts.

**Revisit when**: Input bounds analysis shows wider ranges possible.

### Missing Filetests

**Decision**: Create filetests for worley, random, srandom BEFORE optimizing.

**Why**: These functions currently have no regression safety net.

**Revisit when**: Never - safety first.
