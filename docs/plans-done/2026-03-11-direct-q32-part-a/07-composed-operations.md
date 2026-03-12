# Phase 7: Update Composed Operations

These files use float ops for matrix math, interpolation, and trigonometric
helpers. All are mechanical replacements — the same patterns as phases 3-6,
just in more files.

## `expr/matrix.rs`

Matrix add/sub/mul/div per-element, plus matrix×matrix dot products.
All float ops in this file operate on GLSL float matrix elements.

Replacements:
- `fadd` → `ctx.emit_float_add` (~5 sites)
- `fsub` → `ctx.emit_float_sub` (~3 sites)
- `fmul` → `ctx.emit_float_mul` (~12 sites — matrix multiply is multiply-heavy)
- `fdiv` → `ctx.emit_float_div` (~1 site, matrix / scalar)
- `fcmp` → `ctx.emit_float_cmp` (~1 site, matrix equality)

~22 sites total.

## `builtins/matrix.rs`

Matrix inverse, determinant, outer product, component multiply.
Heavy use of fmul/fsub/fadd/fdiv/f32const.

Replacements:
- `fmul` → `self.emit_float_mul` (~30+ sites)
- `fsub` → `self.emit_float_sub` (~15 sites)
- `fadd` → `self.emit_float_add` (~10 sites)
- `fdiv` → `self.emit_float_div` (~5 sites)
- `f32const` → `self.emit_float_const` (~8 sites: 0.0, 1.0, -1.0)

~70 sites. This is the largest single file. The operations are repetitive
(determinant/inverse formulas expanded element-by-element). Consider doing
a find-and-replace pass with manual verification.

## `builtins/interpolation.rs`

mix(), step(), smoothstep() — all use float arithmetic.

### mix
- `f32const(1.0)` + `fsub` + `fmul` × 2 + `fadd` per component
- ~10 sites

### step
- `f32const(0.0)` + `f32const(1.0)` + `fcmp` + select
- ~4 sites

### smoothstep
- `f32const` (0, 1, 2, 3) + `fsub` + `fdiv` + `fmax` + `fmin` + `fmul`
- ~15 sites

~29 sites total.

## `builtins/trigonometric.rs`

radians() and degrees() — constant × multiply per component.

```rust
// Before:
let pi_over_180 = self.builder.ins().f32const(0.017453292519943295);
result_vals.push(self.builder.ins().fmul(deg, pi_over_180));

// After:
let pi_over_180 = self.emit_float_const(0.017453292519943295);
result_vals.push(self.emit_float_mul(deg, pi_over_180));
```

4 sites (2 constants + 2 multiplies).

## Total: ~125 sites across 4 files

The matrix files account for the majority. All are mechanical.

## Approach

Given the volume in builtins/matrix.rs, the most efficient approach is:

1. Replace `self.builder.ins().fmul(` with `self.emit_float_mul(` globally
   within builtins/matrix.rs
2. Same for fadd, fsub, fdiv, f32const, fcmp
3. Verify compilation
4. Manually review each replacement for correctness (ensure we didn't
   accidentally replace an integer operation)

The `self.builder.ins().` prefix makes these safe to find-and-replace —
integer operations use `iadd`, `isub`, `imul`, etc. which won't match.
