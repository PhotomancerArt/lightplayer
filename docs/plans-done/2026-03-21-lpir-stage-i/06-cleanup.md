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
- Module/function **well-formedness** rules match `00-design.md` (`entry`
  declaration — at most one,
  call targets, control-flow placement, slot refs).
- Numeric semantics: integer div/rem by zero → `0`; saturating `ftoi_sat_*`;
  memcpy non-overlap; safe-memory assumption stated.
- Op names are consistent throughout (no `float.add` remnants vs `fadd`).
- Type names are consistent (`f32` not `float`, `i32` not `int`).
- Slot names use `ssN` everywhere.
- Constants use `iconst.i32` / `fconst.f32` throughout.
- Immediate variants use `_imm` suffix consistently.
- Every op mentioned in the mapping table is defined in `02-core-ops.md`.
- Every `std.math` function in `06-import-modules.md` appears in
  `08-glsl-mapping.md`.
- Import names use `@module::name` syntax consistently; no bare `@__lp_*`
  or `@__lpfx_*` style imports remain.
- No `mathcall` keyword remains anywhere — all external calls use `call`.
- Grammar in `07-text-format.md` covers every op and construct.
- Examples are valid according to the grammar.

### 2. Completeness review

Verify against the current WASM emitter:
- Every handled `Expression` variant has a mapping.
- Every handled `Statement` variant has a mapping (including `Switch`).
- Every binary/unary operator has a mapping for all applicable types.
- Every `Math` function maps to a `std.math` import.
- Q32 is mentioned only as a backend concern, not in the IR.
- `entry func` (not `export func`) used consistently for the runtime entry point.
- Source language target (GLSL 4.50 core) stated in overview.
- Recursion allowed; stack overflow is implementation-defined termination.
- Shadow stack / elision noted in target mapping for WASM slots.
- Import module semantic precision section present (relaxed default for
  `std.math` transcendentals).
- Endianness (little-endian) stated in memory chapter.
- `switch` is first-class control flow (not in future extensions); grammar,
  mapping, and well-formedness rules present.

### 3. Plan cleanup

- Write `summary.md` documenting what was produced.
- Move plan files to `docs/plans-done/`.

### 4. Commit

Commit the spec and plan files:

```
docs(lpir): LPIR language specification

- Complete LPIR spec across docs/lpir/ chapters: type system, ops,
  memory model, calls, control flow, text format grammar, GLSL mapping
- Design decisions: Q32 in emitter, module-qualified imports,
  general pointer model, width-aware VReg types
```
