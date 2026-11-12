# Milestone 1 — Pointer-ABI foundation + array migration

## Goal

Establish the unified pass-by-pointer aggregate ABI end-to-end (frontend
`lower_call`, host `lpvm_abi.rs`, three backend ABI sites), and migrate
the existing array `in` calling convention from flat-scalar to pointer.
At end of M1, the codebase has exactly one aggregate ABI; all existing
array filetests stay green.

## Suggested plan name

`lp-shader-aggregates-m1-pointer-abi-foundation`

## Scope

### In scope

- Unified `AggregateSlot` abstraction in `lp-shader/lps-frontend/src/
  lower_ctx.rs`, replacing the array-only `ArraySlot` enum with a shape
  general enough to also hold struct slots in M2 (without yet adding
  struct support).
- New `lower_call.rs` (or extracted module from `lower.rs`) implementing
  the unified call-site lowering:
  - `in T` aggregate: emit `Address(slot)` for the caller's source slot,
    pass the pointer; callee receives a `Pointer` arg, allocates own
    slot, `Memcpy`s in at function entry.
  - `inout` / `out` aggregate: same pointer pass-through, callee writes
    through.
  - Aggregate return: caller allocates a destination slot, passes its
    address as a hidden first arg; callee writes through; rewrite
    `Statement::Return { value }` lowering for aggregate returns.
- `lp-shader/lps-frontend/src/lower_array.rs` rewrite of the `in` array
  path: `LowerCtx::new` no longer flattens `in T[]` to N scalar VReg
  params; instead it allocates a slot and emits a `Memcpy` from the
  pointer arg at entry. `inout`/`out` array handling stays as-is (it's
  already pointer-based).
- `lp-shader/lps-frontend/src/naga_util.rs` updates: `array_ty_flat_ir_types`
  becomes `array_ty_pointer_arg_ir_type` (returns a single `IrType::Pointer`
  for the param, not a flat scalar list); `func_return_ir_types` becomes
  `func_return_ir_types_with_sret` returning both the regular return
  list and an optional `sret` arg type.
- `lp-shader/lpvm/src/lpvm_abi.rs` aggregate-arg/return code paths
  collapse to `LpvmDataQ32` construction:
  - `flatten_q32_arg` for aggregates: build `LpvmDataQ32::from_value(ty,
    &value)`, return its `as_ptr()` (caller keeps the buffer alive
    across the call).
  - `decode_q32_return` for aggregates: caller pre-allocates
    `LpvmDataQ32::new(ret_ty)`, passes `as_mut_ptr()` as hidden first
    arg, then calls `to_value()` after the call returns.
  - Scalar / vec / mat marshalling stays unchanged.
- `lpvm-native` (cranelift host JIT), `lpvm-cranelift` (RV32 cross), and
  `lpvm-wasm` ABI classifiers updated to consume the new shape: one
  `Pointer` arg per aggregate, one optional `sret` hidden first arg per
  aggregate return. The `scalar_count_of_type` recursion goes away for
  aggregates (replaced by "one pointer per aggregate"); it stays for
  scalars/vec/mat.
- Filetest CHECK-line rewrites for any test that asserts against
  pre-link IR shape involving an array `in` arg. Bulk update with a
  single shape change ("flat scalar args" → "one pointer arg + entry
  Memcpy").
- Update existing rust callers of `lpvm_abi::flatten_q32_arg` /
  `decode_q32_return` to use the new aggregate-by-pointer API. Should
  be limited to lp-shader internals + a small number of test
  harnesses (R3 mitigation).
- Acceptance: full filetest sweep passes on `jit.q32`, `wasm.q32`,
  `rv32c.q32`, and `rv32n.q32` (per Q7). Per-backend RV32 calling
  convention validation explicitly required (R1 mitigation).

### Out of scope

- Struct support (M2).
- Arrays of structs (M3).
- Uniform-with-array-field (M4).
- Read-only-`in` optimisation (M5).
- Small-aggregate register-return fast path (follow-up perf roadmap).

## Key decisions

- **`AggregateSlot` enum shape.** Naming and variant set is settled in
  the M1 plan, but the intent is `AggregateSlot::Local(SlotId) |
  AggregateSlot::Param(arg_i)`, generalising over both arrays and (in
  M2) structs.
- **`sret` representation in LPIR.** Two viable shapes: (a) hidden
  first-arg convention enforced by the frontend, with no LPIR-level
  marker; (b) explicit `IrFunction::sret_arg: Option<VReg>` field that
  the backend reads. Choice settled in the M1 plan; (b) preferred
  because it makes the calling convention self-documenting at the IR
  level and avoids each backend re-discovering the convention.
- **`lower_call.rs` extraction.** Whether to pull call lowering out of
  `lower.rs` into its own module. Settled in the M1 plan; expected to
  go into a new file given how much the call path grows.
- **Filetest CHECK-line rewrite tooling.** Whether to script the bulk
  rewrite or do it by hand. Settled in M1 plan; manual probably fine
  given scope (few dozen tests).

## Deliverables

- Modified files:
  - `lp-shader/lps-frontend/src/lower_ctx.rs` (AggregateSlot, generalised
    slot allocation)
  - `lp-shader/lps-frontend/src/lower_array.rs` (`in` array pointer-ABI
    rewrite)
  - `lp-shader/lps-frontend/src/lower.rs` (call-lowering extraction or
    in-place rewrite)
  - `lp-shader/lps-frontend/src/naga_util.rs` (signature helpers)
  - `lp-shader/lpvm/src/lpvm_abi.rs` (aggregate cases via `LpvmDataQ32`)
  - `lp-shader/lpvm-native/src/abi/classify.rs` and surrounding ABI
    code (one pointer per aggregate, sret hidden arg)
  - `lp-shader/lpvm-cranelift/...` (RV32 calling convention)
  - `lp-shader/lpvm-wasm/...` (i32 pointer args)
  - `lp-shader/lps-filetests/filetests/...` CHECK-line updates
  - Rust call-site migrations for `flatten_q32_arg` /
    `decode_q32_return`
- New file (likely):
  - `lp-shader/lps-frontend/src/lower_call.rs`
- New filetest:
  - At least one filetest that exercises an aggregate (array) return
    via `sret` round-tripped end-to-end on each of `jit.q32`,
    `wasm.q32`, `rv32c.q32`, `rv32n.q32` (R1 mitigation).

## Dependencies

None. M1 is the foundation; all other milestones depend on it.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: M1 touches every layer of `lp-shader` (frontend, IR call
lowering, host ABI marshalling, three backends, plus the existing array
filetest corpus). Multiple architectural sub-decisions need explicit
question iteration: `AggregateSlot` shape, `sret` representation in
LPIR, call-lowering module extraction, filetest update strategy. 4–6
phases of work, with parallelism opportunities (e.g. backend ABI
updates can fan out once the frontend produces the new shape).

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
