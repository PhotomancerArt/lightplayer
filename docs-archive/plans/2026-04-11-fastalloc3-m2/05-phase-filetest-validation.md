# Phase 5: Filetest validation + annotations

## Goal

Run filetests under `rv32fa`, fix any issues, annotate known failures.

## Process

1. Run filetests with `rv32fa` target
2. Identify failures — categorize:
   - Control flow (IfThenElse, Loop) → `unimplemented: rv32fa.q32`
   - Calls → `unimplemented: rv32fa.q32`
   - Params (if fa_alloc doesn't handle them) → `unimplemented: rv32fa.q32`
   - Actual bugs → fix
3. Add annotations to filetest `.glsl` files
4. Confirm all straight-line filetests pass

## Expected outcomes

Straight-line arithmetic filetests (iconst, add, sub, mul, comparisons, etc.)
should match cranelift results exactly since both produce correct RV32 code.

Filetests with control flow or calls will fail with `AllocError` and need
`unimplemented` annotations.

## Status: [ ]
