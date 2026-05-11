# Phase 9: Cleanup and Validation

## Scope

Final validation, profile comparison, and plan completion.

**In scope:**
- Full CI gate
- Profile comparison across all phases
- Plan cleanup and summary
- Single commit for entire plan

**Out of scope:**
- New optimizations
- New file creation (except summary.md)

## Files to Modify

- `docs/plans/2026-04-28-noise-perf-improvements/summary.md` (new)
- Move plan directory: `docs/plans/` → `docs/plans-old/`

## Cleanup Checklist

### Code Quality
```bash
# Search for temporary code
grep -r "TODO\|FIXME\|XXX\|dbg!\|println!" \
  lp-shader/lps-builtins/src/builtins/lpfn/generative/

# Check for unused functions
cargo clippy -p lps-builtins -- -W unused

# Verify no test ignores
rg "#\[ignore" lp-shader/lps-builtins/
```

### Validation Commands

```bash
# 1. Update nightly (CI uses fresh)
rustup update nightly

# 2. Full CI gate
just check       # fmt + clippy host + clippy rv32
just build-ci    # host + rv32 builtins + emu-guest
just test        # cargo test + glsl filetests
```

### Profile Comparison

Collect profiles from all phases:
```bash
ls -la profiles/ | grep examples-perf-fastmath
```

Compare key metrics in each `report.txt`:
- `__lp_lpfn_psrdnoise3_q32` self-cycles
- `__lp_lpfn_snoise2_q32` self-cycles
- `__lp_lpfn_snoise3_q32` self-cycles
- `__lp_lpfn_gnoise2_q32` self-cycles
- `__lp_lpfn_gnoise3_q32` self-cycles
- `__lp_lpfn_worley2_q32` self-cycles
- `__lp_lpfn_worley3_q32` self-cycles
- `__lps_sin_q32` / `__lps_cos_q32` calls (should decrease in psrdnoise3)

Expected trends:
- psrdnoise3: significant reduction (hash + trig + wrapping)
- snoise2/3: moderate reduction (branchless + wrapping)
- gnoise: modest reduction (smoothing LUT)
- worley: minimal reduction (wrapping only)

## Summary Document

Create `docs/plans/2026-04-28-noise-perf-improvements/summary.md`:

```markdown
# Noise Function Performance Improvements - Summary

## What was built

- Created missing filetests for worley, random, srandom functions
- Added snapshot tests for all 10 noise function variants
- **psrdnoise3_q32**: Integer hash chain, Fibonacci gradient LUT, wrapping math, no-period fast path
- **snoise2/3_q32**: Branchless simplex ordering, wrapping surflet math
- **gnoise2/3_q32**: Quintic/cubic smoothing LUTs
- **worley2/3_q32**: Wrapping distance math
- **random/srandom**: Analyzed (minimal sincos opportunity, documented)

## Decisions for future reference

### Integer hash vs saturated Q32

- **Decision**: Use i32 rem_euclid for 289-based hash chains
- **Why**: Saturated Q32 silently overflows, causing drift from GLSL reference
- **Revisit when**: Hash period exceeds i32 safe range (289 is safely within)

### Gradient LUT sizing

- **Decision**: 289 entries for psrdnoise (matches permute period), 256 for smoothing
- **Why**: Algorithm-specific constants; tradeoff between precision and cache
- **Revisit when**: Memory pressure requires smaller tables or quality needs higher precision

### Branchless step complexity

- **Decision**: Implemented for 2D (simple), deferred 3D complexity
- **Why**: 3D has 6 orderings; LUT approach may be better than bit manipulation
- **Revisit when**: snoise3 profiling justifies the complexity

### Wrapping math preconditions

- **Decision**: Only use wrapping where mathematically provable (simplex bounds)
- **Why**: Unbounded wrapping causes visible artifacts
- **Revisit when**: Formal verification of bounds available

## Performance Results

| Function | p0 Baseline | pN Final | Improvement |
|----------|-------------|----------|-------------|
| psrdnoise3 | TBD | TBD | TBD |
| snoise2 | TBD | TBD | TBD |
| snoise3 | TBD | TBD | TBD |
| gnoise2 | TBD | TBD | TBD |
| gnoise3 | TBD | TBD | TBD |
| worley2 | TBD | TBD | TBD |
| worley3 | TBD | TBD | TBD |

## Notes

- All snapshots pass with documented drift where expected (hash algorithm changes)
- All filetests pass
- CI gate clean
```

## Archive Plan

```bash
# Move to plans-old
git mv docs/plans/2026-04-28-noise-perf-improvements \
      docs/plans-old/2026-04-28-noise-perf-improvements
```

## Commit

Single conventional commit for entire plan:
```
perf(lps-builtins): optimize all noise functions with psrdnoise2 patterns

Apply performance patterns from psrdnoise2_q32 to full noise suite:

- psrdnoise3: Integer hash chain, Fibonacci gradient LUT, wrapping math,
  no-period fast path (reduces self-cycles by X%)
- snoise2/3: Branchless simplex ordering, wrapping surflet math
- gnoise2/3: Quintic/cubic smoothing LUTs for interpolation
- worley2/3: Wrapping distance math for cellular features
- random/srandom: Analyzed, minimal opportunity (documented)

Safety:
- Created filetests for worley, random, srandom (previously uncovered)
- Added snapshot tests for all 10 noise function variants
- All existing filetests pass with documented output drift where
  algorithm changes improve correctness (integer hash)

Plan: docs/plans-old/2026-04-28-noise-perf-improvements/
```

## Validate

```bash
# Final full CI gate
rustup update nightly
just ci  # check + build-ci + test

# Verify plan archived
ls docs/plans-old/2026-04-28-noise-perf-improvements/
```

## Definition of Done

- [ ] No TODO/FIXME/dbg!/println! in modified code
- [ ] `just ci` passes
- [ ] `summary.md` created with actual numbers
- [ ] Plan directory moved to `docs/plans-old/`
- [ ] Single commit with conventional format
- [ ] Commit references archived plan path
