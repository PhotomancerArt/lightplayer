# Milestone 2 — Struct lowering on the unified ABI

## Goal

Add first-class GLSL struct support — locals, member access, whole-struct
copy, function args/returns, nested structs — on top of the unified
pass-by-pointer ABI established in M1. At end of M2, every struct-related
filetest in the existing acceptance corpus passes on **`wasm.q32`**,
**`rv32c.q32`**, and **`rv32n.q32`** — the same three targets as
`lps-filetests::DEFAULT_TARGETS` (see `lps-filetests/src/targets/mod.rs`).
**`jit.q32` is not an acceptance target** (host Cranelift convenience only;
CI and milestone gates do not hinge on it). **`rv32c.q32` and `rv32n.q32` must
have parity** (same pass/fail set); any divergence is a backend bug to fix,
not a reason to leave one RV32 path ignored.

## Suggested plan name

`lp-shader-aggregates-m2-struct-lowering`

## Scope

### In scope

- Extend `lp-shader/lps-frontend/src/naga_util.rs::naga_type_to_ir_types`
  with a recursive struct arm that flattens `TypeInner::Struct` to a
  std430-ordered sequence of `IrType`s. Mirror updates to
  `expr_type_inner` and `expr_scalar_kind` to handle struct-member
  access through `AccessIndex`.
- Add `StructInfo` and `struct_map: BTreeMap<Handle<LocalVariable>,
  StructInfo>` to `LowerCtx` (`lp-shader/lps-frontend/src/lower_ctx.rs`).
  `StructInfo` carries the `AggregateSlot` (from M1), std430 total size,
  and a per-member offset+`IrType` table indexed by Naga member index.
- Allocate slots for every struct `LocalVariable` in `LowerCtx::new`,
  regardless of whether it's a real local or a function-param alias.
  For struct `in` params, use the M1 `Memcpy`-from-pointer-arg entry
  shape (no flat-scalar param VRegs, no `param_aliases` entry). For
  `inout`/`out`, register `AggregateSlot::Param(arg_i)` like arrays.
- New module `lp-shader/lps-frontend/src/lower_struct.rs` analogous to
  `lower_array.rs`. Exposes:
  - `zero_fill_struct_slot`
  - `load_struct_member_to_vregs(ctx, info, member_idx)`
  - `store_vregs_into_struct_member(ctx, info, member_idx, vregs)`
  - `memcpy_struct(ctx, dst_addr, src_addr, total_bytes)`
  - `lower_struct_compose_into_slot(ctx, slot_addr, lps_struct_ty,
     compose_components)` — recursive over nested struct components
  - `materialise_struct_rvalue_to_temp_slot(ctx, expr_h)` — for `Compose`
    or call-return expressions appearing as direct call arguments
- Extend `lp-shader/lps-frontend/src/lower_expr.rs`:
  - `Expression::AccessIndex` on a struct local slot (offset + typed
    `Load`) and on a struct via `inout`/`out` param pointer.
  - `Expression::Load` of a struct local — produces the flat VReg
    bundle if the rvalue ends up consumed component-wise; produces a
    temp-slot pointer if consumed as an aggregate.
  - `Expression::Compose` for `LpsType::Struct` writes into a destination
    slot if known, else into a freshly allocated temp slot (R6 mitigation).
- Extend `lp-shader/lps-frontend/src/lower_stmt.rs`:
  - `Statement::Store` whole-struct assignment: `Memcpy` slot-to-slot.
  - `Statement::Store` of a struct member on a local: typed `Store` at
    member offset.
- Extend `lp-shader/lps-frontend/src/lower_access.rs` for struct-member
  stores through an `inout`/`out` param pointer.
- Toggle off `@unimplemented(wasm.q32)`, `@unimplemented(rv32c.q32)`, and
  `@unimplemented(rv32n.q32)` on the struct acceptance corpus (aggressive:
  enable tests that should pass for this milestone so bugs surface):
  - All 9 files in `lp-shader/lps-filetests/filetests/struct/`
  - `lp-shader/lps-filetests/filetests/function/param-struct.glsl`
  - `lp-shader/lps-filetests/filetests/function/return-struct.glsl`
  - `lp-shader/lps-filetests/filetests/uniform/struct.glsl` (the
    items not already passing)
  - `lp-shader/lps-filetests/filetests/global/type-struct.glsl` as needed
- Default is to **fix failures uncovered by un-ignoring** within M2. Only if a
  failure is clearly **orthogonal** to struct lowering (e.g. a pre-existing
  RV32 codegen bug on an op the test happens to touch) may a specific case be
  re-marked with `// TODO(bug-N): …` and a filed issue — not as a blanket
  deferral of RV32.

### Out of scope

- Arrays of structs (M3).
- Uniform-with-array-of-struct (M4).
- Read-only-`in` optimisation (M5).
- Struct equality (`a == b`) — not in the existing filetest corpus.
- Sampler/opaque struct fields.

## Key decisions

- **Acceptance targets.** `wasm.q32`, `rv32c.q32`, `rv32n.q32` only. Not
  `jit.q32`. RV32 Cranelift vs native must stay in lockstep on the struct
  corpus.
- **Storage of struct locals: always slot-backed.** "Aggregates are
  always data" — already settled at the roadmap level. Whole-struct copy
  is `Memcpy`; member access is `Load`/`Store` at std430 offset.
- **Struct rvalue temp-slot lifetime.** When `Compose` appears as a
  direct call argument with no destination slot, M2 allocates a temp
  slot whose lifetime is the enclosing `Statement` (settled in the M2
  plan; LPIR slot lifetimes today don't have explicit scoping, so this
  is "alloc and never reuse"; perf cost is acceptable, optimisable
  later).
- **Nested-`Compose` / slot materialisation.** Use a unified primitive that
  writes a value of a given `LpsType` at a `(base, byte_offset)` into a slot
  (recursing for nested struct and array members), shared with array init
  where practical — not a flat-IR-vreg walk that ignores std430 padding.
- **`param_aliases` for struct args.** Not used. Struct args go through
  the Memcpy-on-entry slot path (consistent with arrays).

## Deliverables

- Modified files:
  - `lp-shader/lps-frontend/src/naga_util.rs` (struct flatten arms)
  - `lp-shader/lps-frontend/src/lower_ctx.rs` (StructInfo, struct_map)
  - `lp-shader/lps-frontend/src/lower_expr.rs` (AccessIndex, Load,
    Compose for struct)
  - `lp-shader/lps-frontend/src/lower_stmt.rs` (Store for struct)
  - `lp-shader/lps-frontend/src/lower_access.rs` (member store
    through pointer)
- New file:
  - `lp-shader/lps-frontend/src/lower_struct.rs`
- Filetest updates: `@unimplemented(wasm.q32)`, `@unimplemented(rv32c.q32)`,
  and `@unimplemented(rv32n.q32)` toggled off across the struct acceptance
  corpus (listed above). `jit.q32` annotations are out of scope for M2
  acceptance.

## Dependencies

- **Requires M1 complete.** The M2 work assumes
  `AggregateSlot`, the unified `lower_call` shape, and the host ABI
  via `LpvmDataQ32` are all in place and that aggregate args/returns
  use pointers consistently.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: M2 introduces a substantial new module
(`lower_struct.rs`), extends four existing lowering modules, and has
several real design questions (struct rvalue temp-slot lifetime,
nested-`Compose` recursion shape, `param_aliases` interaction with
struct args). Worth ~4–5 phases. Parallelism is limited (most phases
build on each other's output) but possible at the boundary between
expression-side and statement-side extensions.

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
