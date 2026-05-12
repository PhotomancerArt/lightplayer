# M2.4 Q32 Float Operations - Plan Notes

## Scope of Work

Implement Q32 float support in lpvm-native via soft-float builtins:

- **Q32 arithmetic**: fadd, fsub, fmul, fdiv (fadd/fsub/fmul already stubbed)
- **Q32 comparisons**: feq, fne, flt, fle, fgt, fge via integer compares
- **Q32 division**: fdiv via `__lp_lpir_fdiv_q32` builtin call
- **Q32 constants**: fconst encoding as Q32 fixed-point (already done)
- **Tests**: Unit tests in lower.rs, filetests for float operations

Out of scope:
- F32 native float (no hardware FPU)
- Math library functions (sin, cos, sqrt)
- 64-bit double operations

## Current State

### Already Implemented

1. **Fadd/Fsub/Fmul lowering** in `lower.rs` (lines 273-299):
   - Lower to `__lp_lpir_fadd_q32`, `__lp_lpir_fsub_q32`, `__lp_lpir_fmul_q32` calls
   - Only when `float_mode == FloatMode::Q32`

2. **FconstF32 lowering** in `lower.rs` (lines 302-309):
   - Converts f32 to Q32 fixed-point: `val * 65536.0` as i32

3. **Builtins exist** in `lps-builtins/src/builtins/lpir/`:
   - `fadd_q32`, `fsub_q32`, `fmul_q32`, `fdiv_q32` - all exist and are tested
   - `fnearest_q32`, `fsqrt_q32` - also exist

4. **Float comparisons in LPIR**: `Feq`, `Fne`, `Flt`, `Fle`, `Fgt`, `Fge` exist in `lpir/src/op.rs`

5. **Cranelift backend reference**: Shows Q32 comparisons use integer compares:
   - `Feq` → `icmp eq`
   - `Flt` → `icmp slt` (signed less than)
   - `Fle` → `icmp sle` (signed less or equal)
   - etc.

### Missing

1. **Fdiv lowering** - needs to call `__lp_lpir_fdiv_q32`
2. **Float comparison lowerings** - all 6 variants need Q32 handling
3. **Unit tests** for new lowerings
4. **Filetests** for Q32 float operations

## Questions

### Q1: How to handle Fne (float not-equal)?

**Context**: The milestone document lists `feq, flt, fle, fgt, fge` but doesn't mention `fne`. However, `lpir/src/op.rs` has `Fne` defined, and the cranelift backend implements it.

**Current state**: Cranelift uses `icmp ne` for Q32 Fne.

**Answer**: Yes, include Fne. We'll implement the full set: Feq, Fne, Flt, Fle, Fgt, Fge.

**Decision**: Include all 6 comparison ops in scope.

### Q2: What error message for F32 mode float ops?

**Context**: Currently fadd/fsub/fmul/fconst return an error "float op in F32 mode (M1: Q32 only for float lowering)" when not in Q32 mode.

**Current state**: The catch-all pattern at line 311 covers Fadd, Fsub, Fmul, FconstF32.

**Answer**: Use clearer error message: "float op requires Q32 mode (F32 not supported on rv32)". Extend catch-all to cover Fdiv + all 6 comparison ops.

**Decision**: Update error message and extend pattern.

### Q3: Should we add comparison builtins instead of using integer ops?

**Context**: Float comparisons could either:
- A) Lower to integer comparisons directly (like cranelift does)
- B) Call builtins like `__lp_lpir_feq_q32`

**Current state**: No comparison builtins exist in `lps-builtins/src/builtins/lpir/`. Cranelift uses integer compares.

**Answer**: Use integer comparisons exactly as cranelift does:
- Feq → Ieq
- Fne → Ine
- Flt → IltS
- Fle → IleS
- Fgt → IgtS
- Fge → IgeS

**Decision**: Match cranelift behavior exactly.

### Q4: Test strategy - what filetests to include?

**Context**: Need to verify Q32 operations work end-to-end.

**Current state**: There are existing GLSL filetests but none specifically for Q32 float ops.

**Answer**: Existing tests already cover Q32 extensively - all float tests in `scalar/float`, `vecX`, and `matX` directories use Q32 mode. No new filetests needed for basic Q32 ops.

**Decision**: Rely on existing float filetests for validation.

### Q5: Should Fdiv handle division by zero?

**Context**: The `__lp_lpir_fdiv_q32` builtin returns 0 for 0/0 per the design docs.

**Current state**: Builtin already handles this case.

**Answer**: No special handling - just call the builtin directly. It handles 0/0 → 0 internally.

**Decision**: Direct builtin call, no special lowering logic.

## Summary of Decisions

1. **Include all 6 comparison ops**: Feq, Fne, Flt, Fle, Fgt, Fge
2. **Clear error message**: "float op requires Q32 mode (F32 not supported on rv32)"
3. **Integer comparisons for Q32**: Match cranelift exactly
4. **No new filetests**: Existing float tests cover Q32
5. **Direct Fdiv builtin call**: No special handling needed

## Notes

- Rainbow (the target use case) uses: fadd, fsub, fmul, fdiv, fcmp (via builtins)
- Q32 encoding: `value = f32 * 65536.0`
- All Q32 builtins follow naming pattern: `__lp_lpir_f<op>_q32`
