# M2 — Struct Lowering: Notes

Working notes for the plan. Roadmap milestone:
`docs/roadmaps/2026-04-22-lp-shader-aggregates/m2-struct-lowering.md`.

## Scope of work

Add first-class GLSL **struct** support to `lps-frontend` on top of the
unified pass-by-pointer aggregate ABI established in M1 (`AggregateSlot`,
`IrFunction::sret_arg`, `LpvmDataQ32`-based host marshalling). At end of M2:

- The M2 struct corpus passes on **`wasm.q32`**, **`rv32c.q32`**, and
  **`rv32n.q32`** (`lps-filetests::DEFAULT_TARGETS`). **`jit.q32` is not an
  acceptance target.** `rv32c` and `rv32n` must have the same pass/fail set.
  Remove the relevant `// @unimplemented(wasm|rv32c|rv32n).q32` markers
  (aggressive enablement; fix uncovered bugs in M2 unless clearly orthogonal).
  Corpus:
  - `lps-filetests/filetests/struct/*.glsl` (9 files: `assign-simple`,
    `constructor-nested`, `constructor-simple`, `constructor-vectors`,
    `define-nested`, `define-vector`, plus existing siblings).
  - `lps-filetests/filetests/function/param-struct.glsl`,
    `function/return-struct.glsl`.
  - `lps-filetests/filetests/uniform/struct.glsl` and
    `global/type-struct.glsl` (the not-yet-passing items).

Mechanically (subject to design phase):

- New `lps-frontend/src/lower_struct.rs` mirroring `lower_array.rs`.
- `LowerCtx` extended so its `aggregate_map` covers struct locals/params
  (`AggregateInfo` carries an optional struct-layout side-table; arrays keep
  their existing `dimensions/leaf_*` fields).
- `naga_util.rs::naga_type_to_ir_types` learns `TypeInner::Struct` (flatten
  to std430-ordered scalar IR types). `expr_type_inner` /
  `expr_scalar_kind` learn the `AccessIndex` arms for *value* structs (today
  only the `Pointer→Struct` global-uniform arm exists).
- `lower_expr.rs`:
  - `Expression::AccessIndex` on a struct local (typed `Load` at the
    member offset) and on a struct via `inout`/`out` param pointer.
  - `Expression::Compose` for `LpsType::Struct` writes into a destination
    slot when one is known, else into a freshly allocated temp slot
    (R6 mitigation).
  - `Expression::Load` of a whole struct local should *not* fall through
    to `naga_type_to_ir_types` and produce a flat-vreg bundle. Every
    consumer that today special-cases `aggregate_map` for arrays will also
    special-case it for structs first.
- `lower_stmt.rs::Statement::Store`:
  - whole-struct assignment → `Memcpy` slot↔slot (mirrors
    `copy_stack_array_slots`).
  - per-member store on a struct local → typed `Store` at the member
    offset.
- `lower_access.rs` for struct-member stores through `inout`/`out` param
  pointer.
- `lower_call.rs`: extend `record_call_result_aggregate` and
  `write_aggregate_return_into_sret` to recognise struct returns; extend the
  `TypeInner::Pointer { base: Struct }` and "struct by value `in`" cases in
  `lower_user_call`'s arg loop. Add struct-rvalue temp-slot materialisation
  for `Compose` / call-result expressions appearing as direct call
  arguments.
- Filetest annotation toggles via `--fix` (acceptance gate).

**Out of scope** (settled by milestone doc):

- Arrays-of-structs (M3).
- Uniform with array-of-struct field (M4).
- Read-only-`in` perf optimisation (M5).
- Struct equality (`==` / `!=`).
- Sampler / opaque struct fields.

---

## Current state of the codebase

### What already exists and works

- **Std430 layout for structs.** `lower_aggregate_layout::aggregate_size_and_align`
  accepts struct handles via `naga_types::naga_type_handle_to_lps`
  (`std430_struct_vec3_float` test passes). `lps_shared::layout` does std430
  member offsets/alignment for `LpsType::Struct`.
- **Uniform-struct member loads.** `lower_expr.rs::AccessIndex` handles
  `TypeInner::Pointer { base: Struct } → GlobalVariable` via
  `load_lps_value_from_vmctx`, which already recurses into
  `LpsType::Struct`.
- **Unified pass-by-pointer ABI plumbing (M1).** For arrays:
  - `AggregateSlot::{Local, Param}` and `AggregateInfo` in `lower_ctx.rs`.
  - `IrFunction::sret_arg` plus `SretCtx` in `LowerCtx`, written in
    `LowerCtx::new` via `func_return_ir_types_with_sret`.
  - `lower_call.rs` handles aggregate args/returns end-to-end for arrays:
    caller `aggregate_storage_base_vreg` → pointer arg; sret slot allocated
    by caller; callee `Memcpy` from arg-pointer into local slot at entry.
  - Contiguous-param-vreg fix landed so `user_param_vregs` and `sret_arg`
    line up for sret functions.
- **`naga_util` partial struct awareness.** `expr_type_inner` and
  `expr_scalar_kind` AccessIndex arms already match
  `TypeInner::Pointer { base: Struct }` (used by global uniform struct
  member access). The helpers are structured to extend.

### What does **not** work today (the M2 gap)

- `naga_type_to_ir_types(TypeInner::Struct {..})` → `UnsupportedType
  ("unsupported type for LPIR")`.
- `LowerCtx::new` only allocates aggregate slots for `TypeInner::Array`
  locals/params; struct locals would fall through to a generic vreg path
  that immediately fails.
- `func_return_ir_types_with_sret` only sret-routes `TypeInner::Array`
  returns. A struct return falls through to `naga_type_to_ir_types(inner)`
  (which fails) — sret is never reached.
- `lower_call::lower_user_call` arg loop only handles `TypeInner::Pointer`
  and `TypeInner::Array`; a struct value `in` arg falls into the scalar
  fallback.
- `lower_call::write_aggregate_return_into_sret` and
  `record_call_result_aggregate` are written against arrays
  (`flatten_array_type_shape`, `aggregate_map`). Need a struct variant or a
  unified one.
- `lower_stmt::Statement::Store` for `LocalVariable` only treats locals as
  arrays-or-flat-vregs.
- `Expression::Compose` for a struct concatenates into a flat `VRegVec`
  with no slot destination; nothing routes that into a `Memcpy`-friendly
  shape.
- `Expression::AccessIndex` on a value local struct
  (`Pointer → Struct` from `LocalVariable`) is unhandled.

### Reference shapes already established

- `lower_array.rs::aggregate_storage_base_vreg`, `zero_fill_array_slot`,
  `lower_array_initializer`, `copy_stack_array_slots` — templates for the
  struct equivalents.
- `lower_call.rs::record_call_result_aggregate` /
  `write_aggregate_return_into_sret` — templates for struct-aware versions.
  Likely cleaner to **promote both to operate on `AggregateInfo` produced
  from any aggregate Naga type** so the array and struct paths converge.
- `lower_expr.rs::load_lps_value_from_vmctx` already recurses through
  `LpsType::Struct` for member offsets — same std430 rules apply for
  slot-backed structs (offsets are identical).

---

## Open questions (to resolve before design)

### Confirmation-style batch (Q1–Q6)

| #   | Question                                                                                                                                                                                          | Context                                                                                                                                                                                                          | Suggested answer                                                                              |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Q1  | Reuse `AggregateInfo` (extend) for both arrays and structs rather than introducing a separate `StructInfo`.                                                                                       | `slot`/`total_size` fields are identical; array-only fields (`dimensions`, `leaf_*`, `element_count`) can be neutral defaults for structs. Avoids parallel maps and parallel call-result handling.               | Yes — extend `AggregateInfo` with an `AggregateKind { Array{..}, Struct{..} }` payload.       |
| Q2  | Cache per-member offset+IR types on the struct payload at slot-allocation time (rather than recomputing from `LpsType::Struct` per access).                                                       | Member offsets are stable per type handle; matches `dimensions/leaf_stride` precedent for arrays.                                                                                                                | Yes.                                                                                          |
| Q3  | Module name `lower_struct.rs` (sibling of `lower_array.rs`).                                                                                                                                      | Mirrors the milestone doc.                                                                                                                                                                                       | Yes.                                                                                          |
| Q4  | Treat `Load(struct local)` as an unsupported `lower_expr_vec` case; require every consumer to dispatch on `aggregate_map` (struct kind) **before** calling `ensure_expr_vec`.                     | Mirrors how arrays work today (`Load(array local) → UnsupportedExpression` is the existing guard). Avoids a flat-vreg fallback that hides ABI mistakes.                                                          | Yes.                                                                                          |
| Q5  | Struct rvalue temp-slot lifetime: "alloc and never reuse" within a function (no slot reuse).                                                                                                      | Already settled in milestone doc.                                                                                                                                                                                | Yes.                                                                                          |
| Q6  | Filetest annotation toggle scope: M2 toggles off `wasm.q32`, `rv32c.q32`, and `rv32n.q32` on the struct corpus (not `jit.q32`). Use `--fix` to clean; fix failures in M2.                         | Aligns with `DEFAULT_TARGETS` and roadmap M2 acceptance.                                                                                                                                                        | Yes (superseded wording — see roadmap).                                                       |

### Discussion-style queue (asked one at a time after Q1–Q6)

- **Q7** — `param_aliases` exclusion for struct value `in` params and the
  entry `Memcpy`-from-arg-pointer. Mirrors arrays exactly; calling out
  because `scan_param_argument_indices` is the only place this invariant
  lives.
- **Q8** — **Settled:** `aggregate_layout(module, ty)` (single layout query);
  `func_return_ir_types_with_sret` and call/ctx sites consume it.
- **Q9** — **Settled:** unified `store_lps_value_into_slot` (or equivalent)
  for `(base, offset, LpsType, expr)`; padding-aware; shared with array init
  where practical.
- **Q10** — **Settled:** acceptance = `wasm` + both RV32 paths, parity
  required; aggressive un-ignore; see updated roadmap (not M6-defer-by-default).

---

## Notes (running)

- Roadmap `m2-struct-lowering.md` updated: `jit.q32` dropped as acceptance
  target; `wasm` / `rv32c` / `rv32n` are the M2 gates; RV32 parity required.
- Plan phase write-ups: `01-design.md` plus `02-aggregate-layout-refactor.md`
  … `06-enable-and-validate.md`. GLSL filetests run via
  `scripts/glsl-filetests.sh` (or `cargo run -p lps-filetests-app --bin
  lps-filetests-app -- test …` from `lp-shader/`), not the ignored
  `cargo test -p lps-filetests --test filetests` harness.
