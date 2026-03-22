# Phase 6: Cleanup and Review

## Scope

Review all `docs/lpir/` chapters for consistency and completeness, then
clean up the plan.

## Reminders

- Cross-reference all chapters for consistency.
- The spec is the reference for Stage II implementation — it must be
  precise enough that implementation is mechanical.

## Implementation details

### 1. Consistency review

Check across all `docs/lpir/` chapters for:
- Op names are consistent throughout (no `float.add` remnants vs `fadd`).
- Type names are consistent (`f32` not `float`, `i32` not `int`).
- Slot names use `ssN` everywhere.
- Constants use `iconst.i32` / `fconst.f32` throughout.
- Immediate variants use `_imm` suffix consistently.
- Every op mentioned in the mapping table is defined in `02-core-ops.md`.
- Every MathFunc in `06-mathcall.md` appears in `08-glsl-mapping.md`.
- Grammar in `07-text-format.md` covers every op and construct.
- Examples are valid according to the grammar.

### 2. Completeness review

Verify against the current WASM emitter:
- Every handled `Expression` variant has a mapping.
- Every handled `Statement` variant has a mapping.
- Every binary/unary operator has a mapping for all applicable types.
- Every `Math` function has a MathFunc mapping.
- Q32 is mentioned only as a backend concern, not in the IR.

### 3. Plan cleanup

- Write `summary.md` documenting what was produced.
- Move plan files to `docs/plans-done/`.

### 4. Commit

Commit the spec and plan files:

```
docs(lpir): LPIR language specification

- Complete LPIR spec across docs/lpir/ chapters: type system, ops,
  memory model, calls, control flow, text format grammar, GLSL mapping
- Design decisions: Q32 in emitter, mathcall for builtins,
  general pointer model, width-aware VReg types
```
