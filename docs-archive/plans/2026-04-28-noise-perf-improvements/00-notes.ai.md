# Noise Function Performance Improvements - Notes

## Scope

Apply performance patterns from `psrdnoise2_q32` optimizations to all noise functions:
- psrdnoise3_q32 (highest priority - same algorithm family)
- snoise2_q32, snoise3_q32
- gnoise2_q32, gnoise3_q32
- worley2_q32, worley3_q32
- fbm2_q32, fbm3_q32 (compositional - optimize leaf nodes first)
- random2_q32, random3_q32, srandom variants

## Current State of Codebase

### Existing Optimizations (psrdnoise2_q32)
From commits f067c903 through 265cf851:

1. **Integer hash chain**: `hash_corner()` using `i32` `rem_euclid(289)` instead of saturated Q32 mul
2. **Q32 micro-ops**: `half()`, branchless `step()`, `noperiod` fast path split
3. **Combined sincos**: `__lps_sincos_q32` for paired sin+cos at same angle
4. **Gradient angle LUT**: 289-entry precomputed cos/sin table + alpha rotation
5. **Wrapping math**: `mul_wrapping()` / `add_wrapping()` for provably bounded operations

### Noise Function Inventory (14 Q32 implementations)

| Function | Hash Pattern | Trig Usage | Period Support | Optimization Potential |
|----------|--------------|------------|----------------|----------------------|
| psrdnoise2_q32 | ✅ Integer 289 | ✅ LUT-based | ✅ | Baseline (optimized) |
| psrdnoise3_q32 | ⚠️ Q32 289 | ⚠️ Vector sin/cos | ✅ | **HIGH** - same family |
| snoise2_q32 | ❌ noiz bit-mix | ❌ None | ❌ | MEDIUM - branchless step, wrapping math |
| snoise3_q32 | ❌ noiz bit-mix | ❌ None | ❌ | MEDIUM - branchless step, wrapping math |
| gnoise2_q32 | ❌ sin-based | ❌ None | ❌ | LOW-MEDIUM - LUT for smoothing |
| gnoise3_q32 | ❌ sin-based | ❌ None | ✅ (tile) | LOW-MEDIUM - LUT for smoothing |
| worley2_q32 | ❌ noiz bit-mix | ❌ None | ❌ | LOW - wrapping distance math |
| worley3_q32 | ❌ noiz bit-mix | ❌ None | ❌ | LOW - wrapping distance math |
| fbm2_q32 | N/A (delegates) | N/A | ❌ | N/A - optimize snoise |
| fbm3_q32 | N/A (delegates) | N/A | ✅ (tile) | N/A - optimize snoise/gnoise |
| random2_q32 | ⚠️ sin-based | ✅ Heavy | ❌ | MEDIUM - use sincos |
| random3_q32 | ⚠️ sin-based | ✅ Heavy | ❌ | MEDIUM - use sincos |
| srandom* | ⚠️ sin-based | ✅ Heavy | ❌ | MEDIUM - use sincos |

### Existing Filetests

| Function | Filetest Exists | Coverage |
|----------|----------------|----------|
| psrdnoise | ✅ lp_psrdnoise.glsl | 2D/3D, broadcast periods |
| snoise | ✅ lp_simplex{1,2,3}.glsl | Basic, range, seeds |
| gnoise | ✅ lp_gnoise.glsl | 1D/2D/3D, tile, smoothing |
| fbm | ✅ lp_fbm.glsl | 2D/3D, tile, octaves |
| worley | ❌ None | Missing - needs creation |
| random | ❌ None | Missing - needs creation |
| srandom | ❌ None | Missing - needs creation |

### Profile Target

Baseline: `examples/perf/fastmath` (frozen copy of basic example)
- Each phase: `cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p<N>-<phase>`
- Compare self-cycles in `report.txt` for relevant builtins

## Questions & Decisions

### Q1: Which functions to prioritize?

**Suggested**: Order by similarity to psrdnoise2 + expected impact:
1. psrdnoise3_q32 (same 289 hash pattern, needs trig LUT)
2. snoise2_q32 (widely used, branchless step opportunity)
3. snoise3_q32 (snoise2 patterns applied)
4. gnoise2/3_q32 (smoothing function LUTs)
5. worley2/3_q32 (wrapping math for distances)
6. random/srandom (sincos optimization)

### Q2: Create missing filetests first or optimize first?

**Suggested**: Create filetests first (Phase 0). Safety net before changing algorithms.

### Q3: Should fbm get its own optimizations?

**Suggested**: No - fbm delegates to snoise/gnoise. Optimize the primitives, fbm benefits automatically.

### Q4: How to handle output drift?

**Suggested**: Document expected drift in commit messages. Snapshots must be regenerated when:
- Integer hash replaces saturated Q32 hash (different overflow behavior)
- Trig evaluation order changes (Taylor series differences)

## Key Files to Modify

### Core Q32 library
- `lp-shader/lps-q32/src/q32.rs` - Ensure `mul_wrapping`, `add_wrapping`, `half` available

### Builtin implementations
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/psrdnoise/psrdnoise3_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/snoise/snoise2_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/snoise/snoise3_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/gnoise/*.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/worley/*.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/random/*.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/srandom/*.rs`

### LUT generation (if needed)
- New: `lp-shader/lps-builtins/src/builtins/lpfn/generative/snoise/grad_lut_q32.rs`

### Tests
- `lp-shader/lps-builtins/tests/lpfn_q32_snapshots.rs` - Add snapshots for all functions
- `lp-shader/lps-filetests/filetests/lpfn/` - Create missing filetests

## Validation Requirements

Every phase must:
1. `cargo test -p lps-builtins` pass (snapshot tests)
2. `scripts/filetests.sh --target jit.q32 --target rv32c.q32` pass
3. Profile shows improvement or neutral (no regressions)
4. `just check` clean (no warnings)
