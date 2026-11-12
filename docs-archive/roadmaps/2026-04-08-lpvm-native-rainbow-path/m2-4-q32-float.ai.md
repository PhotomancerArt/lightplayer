# Milestone 2.4: Q32 Float Operations

**Goal**: Complete Q32 float support via soft-float builtins.

## Suggested Plan

`lpvm-native-m2-4-q32-float`

## Scope

### In Scope

- **Q32 arithmetic**: fadd, fsub, fmul, fdiv (already have fadd/fsub/fmul stubs)
- **Q32 division**: fdiv via builtin
- **Q32 comparisons**: feq, flt, fle, fgt, fge via builtins
- **Q32 constants**: fconst encoding as Q32 fixed-point
- **Builtin linking**: Runtime provides __lp_lpir_f* symbols

### Out of Scope

- F32 native float (no hardware FPU in current target)
- Float math library (sin, cos, sqrt — not in rainbow)
- 64-bit double operations

## Key Decisions

1. **Soft-float only**: All float ops lowered to Q32 builtin calls
2. **Builtin naming**: `__lp_lpir_f<op>_q32` pattern (fadd, fsub, fmul, fdiv, feq, flt, etc.)
3. **Q32 encoding**: f32 × 65536.0 → i32 (already done for constants)
4. **Comparison returns**: Bool as i32 (1/0) from builtin, use with select

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| Extended `lower_op` | `lower.rs` | fdiv, fcmp for Q32 |
| Float comparisons | `lower.rs` | Lower fcmp to Q32 builtin calls |
| Q32 division | `lower.rs` | fdiv → __lp_lpir_fdiv_q32 call |
| Runtime builtins | `lpvm-rt` or fw | Provide Q32 math builtins |
| Tests | `lower.rs` | Float op lowering tests |
| Filetests | `filetests/` | float-math.glsl, q32-ops.glsl |

## Dependencies

- M2.3: Function calls (needed to call Q32 builtins)
- M2.1: Comparisons (fcmp feeds into select)

## Estimated Scope

- **Lines**: ~150-250
- **Files**: 2-3 modified (`lower.rs`, plus runtime builtin provision)
- **Time**: 1 day

## Acceptance Criteria

1. All Q32 float ops lower to builtin calls
2. Float comparisons work and integrate with select
3. Q32 division produces correct results
4. Rainbow's smoothstep/mix work via fmul/fsub/select
5. No regressions in existing functionality

## Notes

- Rainbow uses: fadd, fsub, fmul, fdiv, fcmp (via builtins)
- The gradient builtin is a runtime function, not a math op
- Q32 is fixed-point: value = f32 × 65536.0
