# Simplex Noise Artifacts Fix Plan

## Overview

This plan addresses critical bugs in the simplex noise implementation that cause visible artifacts (
diagonal lines, sharp transitions, jagged edges). The issues stem from sign errors in offset
calculations for 2D and 3D simplex noise.

## Problem

The simplex noise functions exhibit visible artifacts including:

- Diagonal lines cutting across the noise pattern
- Sharp transitions between regions
- Jagged edges at cell boundaries
- Lack of smooth interpolation

## Root Cause

**Critical sign errors** in offset calculations:

- 2D: `offset2` calculation subtracts `UNSKEW_FACTOR_2D` when it should add it
- 3D: `offset2` calculation subtracts `UNSKEW_FACTOR_3D` when it should add it

This causes incorrect positioning of simplex corners, leading to discontinuities at cell boundaries.

## Solution Plan

### Phase 1: Fix Critical Bugs

1. Fix sign errors in 2D offset calculation
2. Fix sign errors in 3D offset calculation
3. Verify fixes with existing tests and visual inspection

### Phase 2: Add Testing Infrastructure

1. Add `test_hash_fixed` feature for deterministic testing
2. Create test hash functions for reproducible tests
3. Add boundary continuity tests
4. Add known-value tests (after generating reference values)

### Phase 3: Visual Verification

1. Add visual regression test infrastructure
2. Generate noise images for inspection
3. Add comparison tests against noise-rs
4. Add discontinuity detection tests

## Files to Modify

1. `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise2.rs`
2. `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise3.rs`
3. `lp-glsl/lp-glsl-builtins/Cargo.toml` (add features)
4. `lp-glsl/lp-glsl-builtins/src/builtins/shared/lpfx_hash.rs` (conditional compilation)
5. New: `lp-glsl/lp-glsl-builtins/src/builtins/shared/test_hash.rs`

## Success Criteria

- All existing tests pass
- Visual artifacts are eliminated
- Noise output is smooth and continuous
- Comprehensive test coverage with fixed hash values
- Visual regression tests catch future issues

## Estimated Effort

- Phase 1: 30 minutes (simple sign fixes)
- Phase 2: 2-3 hours (testing infrastructure)
- Phase 3: 1-2 hours (visual tests)

Total: ~4-6 hours

## Dependencies

- No external dependencies required for fixes
- `noise` crate already available for comparison tests (test-only)
- Standard library sufficient for visual tests (PPM format)
