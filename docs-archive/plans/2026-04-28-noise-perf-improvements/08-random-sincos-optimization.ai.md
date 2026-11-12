# Phase 8: random/srandom - Combined Sincos Optimization

## Scope

Optimize random/srandom functions that use sin-based hashing:
1. Replace separate sin+cos calls with combined sincos where both used
2. Apply existing `__lps_sincos_q32` pattern

**In scope:**
- Identify sites where sin and cos of same angle are computed
- Replace with combined sincos call

**Out of scope:**
- Changing hash algorithm (sin-based is fundamental to these functions)
- LUT approaches (random needs full sin precision)

## Files to Modify

- `lp-shader/lps-builtins/src/builtins/lpfn/generative/random/random2_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/random/random3_q32.rs`
- `lp-shader/lps-builtins/src/builtins/lpfn/generative/srandom/srandom*_q32.rs`

## Implementation Details

### Random Function Analysis

Random functions use sin of dot product for hash:
```rust
// random2_q32
let sin_val = __lps_sin_q32(dot_product);
Q32::from_fixed(sin_val)  // Scale to output range
```

Srandom functions are similar but may use sin for vector generation:
```rust
// srandom3_vec_q32
let sin_x = __lps_sin_q32(dot_x);
let sin_y = __lps_sin_q32(dot_y);
let sin_z = __lps_sin_q32(dot_z);
// Returns vec3 of sin values
```

### Sincos Opportunities

Check if any function computes both sin and cos of same angle:

1. **srandom3_vec_q32**: Likely generates 3 different outputs from 3 different inputs (dot_x, dot_y, dot_z) - NOT same angle

2. **Any function doing**: `sin(theta)` and `cos(theta)` from same `theta`:
   - Search for patterns where both trig functions use identical argument
   - If found: replace with `__lps_sincos_q32`

### Investigation Required

Need to read current implementations to identify opportunities:
```bash
grep -n "__lps_sin\|__lps_cos" lp-shader/lps-builtins/src/builtins/lpfn/generative/{random,srandom}/*.rs
```

If no paired sin+cos found, this phase may be minimal.

### Alternative: LUT for Random

If random functions are hot, consider:
- 256 or 512-entry sin LUT (periodic, so can wrap)
- sin is periodic with 2π, so LUT index = (angle * LUT_SIZE / 2π) % LUT_SIZE

Tradeoff: 256-entry sin LUT = 1KB, gives ~0.025 precision
Random quality may suffer with LUT quantization.

**Decision**: Only pursue if profile shows random builtins are significant. fastmath profile likely doesn't use random heavily.

## Validate

```bash
# 1. Check for sincos opportunities
grep -n "sin\|cos" lp-shader/lps-builtins/src/builtins/lpfn/generative/{random,srandom}/*.rs

# 2. If changes made: unit tests
cargo test -p lps-builtins random srandom

# 3. Filetests (lp_random.glsl, lp_srandom.glsl created in Phase 1)
scripts/filetests.sh --target jit.q32 lp_random
scripts/filetests.sh --target jit.q32 lp_srandom

# 4. CI gate
just check
```

## Definition of Done

- [ ] Analysis complete: document which random functions use trig
- [ ] Any paired sin+cos replaced with sincos
- [ ] Filetests pass
- [ ] Snapshots pass without regeneration
- [ ] `just check` clean

## Notes

Random/srandom are typically NOT the hot path in noise shaders (unlike psrdnoise/snoise). If Phase 8 shows minimal opportunity, document and move on - focus effort on higher-impact functions.
