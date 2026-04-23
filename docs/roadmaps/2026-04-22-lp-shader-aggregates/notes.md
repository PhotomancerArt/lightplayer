# lp-shader Aggregates Roadmap — Notes

## Scope

Land a unified, **pass-by-pointer** ABI for *all* composed (aggregate) types
in `lp-shader`: structs, arrays, and arrays-of-structs. Today arrays are
passed flat-by-value, structs aren't supported at function boundaries at
all, and arrays-of-structs are unsupported everywhere outside uniform
buffers. This roadmap collapses those three shapes into one ABI and adds
the language-feature support that makes them usable from GLSL.

The motivating constraint is **domain work** that needs first-class struct
support: GLSL programs that define `struct`s, pass them between functions,
return them, and store arrays of them in uniforms. The choice to make this
a roadmap (rather than a single struct-only plan) was driven by realising
that picking a struct ABI without simultaneously aligning the existing array
ABI would lock us into asymmetry: arrays go flat-by-value, structs go
by-pointer (or vice versa), with arrays-of-structs forced into a third
hybrid path.

Primary user-facing outcome: every existing `@unimplemented` filetest under
`struct/`, `function/{param,return}-struct.glsl`, `uniform/struct.glsl`,
and `const/array-size/struct-field.glsl` passes on `jit.q32`, `wasm.q32`,
`rv32c.q32`, and `rv32n.q32`. Existing array tests remain green through
the migration.

### In scope

- A single **pass-by-pointer aggregate ABI** for arrays and structs:
  - `in T` aggregate: caller passes pointer to a slot; callee allocates its
    own slot and `Memcpy`s in (or, with the optimisation pass, reuses the
    caller's slot when read-only).
  - `inout`/`out T` aggregate: caller passes pointer to a slot; callee
    writes through it.
  - `T` return: caller allocates a destination slot and passes its address
    as a hidden `sret` arg; callee writes through it. No flat-scalar
    aggregate returns.
- **Struct support**: locals, `Compose` (constructor), `AccessIndex`
  (member read/write), whole-struct copy (`Memcpy`), function args/returns,
  nested struct members.
- **Array ABI migration**: existing array `in` params move from flat-scalar
  to pointer; existing tests stay green; same `Memcpy`-on-entry pattern as
  structs.
- **Arrays of structs**: leaf-element-type extends from "scalar/vec/mat" to
  include `LpsType::Struct`; `Particle ps[8];` becomes a stack slot of
  `8 * sizeof(Particle)` bytes addressed by member-offset + element-stride.
- **Uniform structs containing array fields**: extend
  `load_lps_value_from_vmctx` to recurse through array members at
  std430-strided offsets (`uniform { Light lights[8]; }`).
- **Read-only `in` optimisation**: pre-scan for "callee writes nothing
  through this `in` param", elide the entry-`Memcpy` and let the callee use
  the caller's slot directly. Applies uniformly to all aggregates.
- **Host ABI marshalling** (`lpvm_abi::flatten_q32_arg` /
  `decode_q32_return`): rust callers pass aggregate args by writing into a
  buffer and passing its pointer; aggregate returns come back via the
  caller-allocated dest buffer.
- **Backend ABI alignment**: `lpvm-native` (cranelift host JIT),
  `lpvm-cranelift` (RV32 cross-compile), `lpvm-wasm`. The three backends
  see a single uniform pattern — a pointer arg per aggregate, possibly a
  hidden `sret` pointer per aggregate return.

### Out of scope

- Struct equality / inequality (`a == b`). Not in the existing filetest
  corpus; defer to a follow-up.
- Sampler/opaque struct fields.
- A full GLSL std140 layout path. Existing std430 path is the only target.
- Optimisations beyond the read-only-`in` skip-Memcpy pass (e.g. small-
  aggregate fast-path returning in registers, return-value-optimisation
  across nested calls). The unified ABI is correct but not maximally
  fast for tiny aggregates; that perf work is a follow-up roadmap.

## Current state of the codebase

### Aggregate type representation

- **`LpsType::Struct { name, members }`** in `lp-shader/lps-shared/src/
  types.rs` is the canonical struct type with std430 `type_size`,
  `type_alignment`, and `offset_for_path` helpers.
- **`naga_types::naga_type_handle_to_lps`** maps Naga `TypeInner::Struct`
  to `LpsType::Struct` (recursive).
- **`LpsValueQ32::Struct` / `LpsValueF32::Struct`** exist on the host side
  with working `lps_value_f32_to_q32` round-trips.

### Current array ABI (the thing we're migrating)

- Array locals: always slot-backed in `ArrayInfo` (`lower_ctx.rs:53`),
  with `ArraySlot::Local(SlotId)` or `ArraySlot::Param(arg_i)`. Slot
  allocation in `LowerCtx::new` (`:153`).
- Array `in` params: **flat scalars by value** today. `arg_vregs`
  receives one VReg per element scalar. The corresponding `LocalVariable`
  is *also* slot-backed (`scan_param_argument_indices` filter excludes
  arrays from `param_aliases`), so an entry-store path copies flat-arg
  scalars into the slot. Component-wise, not Memcpy.
- Array `inout`/`out` params: **single pointer arg**. `pointer_args`
  records the pointee `Handle<Type>`. Member access through the pointer
  uses the existing `ArraySubscriptRoot::Param(arg_i)` path
  (`lower_ctx.rs:299`).
- Array returns: GLSL doesn't allow them — non-issue.

### Current struct support (the thing we're filling in)

- **Uniforms / globals** containing struct fields work end-to-end on
  `jit.q32` today. `load_lps_value_from_vmctx` recurses through struct
  members at std430 offsets. `global/type-struct.glsl` is *not* tagged
  `@unimplemented(jit.q32)`.
- **Locals, params, returns of struct type** — completely unsupported.
  The blocker is `naga_util::naga_type_to_ir_types` (`:36`) which errors
  on `TypeInner::Struct`. Every code path that asks "what flat IR types
  does this Naga type produce?" goes through it.
- **Host ABI marshalling**: `lpvm_abi::flatten_q32_arg` (`:234`) and
  `decode_q32_return` (`:284`) explicitly return `Unsupported` for
  `LpsType::Struct`.
- **Component-count helpers** already recurse through `LpsType::Struct`
  correctly: `glsl_component_count`,
  `lpvm-native/abi/classify::scalar_count_of_type`,
  `lower::lps_scalar_component_count`. So *count* is right; *flatten* is
  missing.

### Backend ABI plumbing

- `lpvm-native` uses cranelift; `scalar_count_of_type` already iterates
  struct members, so the host-JIT calling convention for "many flat scalar
  args" already handles structs in *theory*. With the new pointer-ABI, the
  classifier becomes much simpler: one pointer per aggregate.
- `lpvm-cranelift` cross-compile to RV32 follows the same pattern. The
  RV32 calling convention puts first 8 args in `a0`–`a7`, rest spill to
  stack — straightforward with pointer ABI (one register per aggregate
  arg).
- `lpvm-wasm` produces wasm imports/exports. Aggregate-as-pointer
  matches wasm's preference for `i32` pointer args into linear memory.

### Acceptance suite already exists

- 9 files in `lp-shader/lps-filetests/filetests/struct/` (define, access,
  constructor, assign — scalar, vector, nested variants).
- `lp-shader/lps-filetests/filetests/function/param-struct.glsl` (in,
  inout, out, const, mixed qualifiers).
- `lp-shader/lps-filetests/filetests/function/return-struct.glsl`.
- `lp-shader/lps-filetests/filetests/uniform/struct.glsl`
  (`jit.q32`-passing for top-level globals; `wasm.q32` / `rv32*` still
  unimplemented).
- `lp-shader/lps-filetests/filetests/const/array-size/struct-field.glsl`
  (struct-with-array-field, arrays-of-structs adjacent).
- All tagged `@unimplemented(jit.q32, wasm.q32, rv32c.q32, rv32n.q32)`
  except where noted, providing a clear acceptance gate per roadmap
  milestone.

## Architectural thesis

> **All composed types pass by pointer to a slot. Optimisation skips the
> entry-Memcpy for `in` params that the callee proves it never writes.**

Concretely:

| Position           | ABI                                    |
| ------------------ | -------------------------------------- |
| `in` aggregate     | `Pointer` arg → callee `Memcpy` into own slot (skip if read-only) |
| `inout` aggregate  | `Pointer` arg → callee writes through  |
| `out` aggregate    | `Pointer` arg → callee writes through  |
| Aggregate return   | Hidden `Pointer` first arg (`sret`) → callee writes through |
| Scalar / vec / mat | Unchanged — flat scalar VRegs by value, scalar return |

Local aggregates are always slot-backed (the "structs are always data"
decision from the abandoned plan applies to arrays too — they already are).
Cross-call ABI becomes a uniform "address of slot" pattern.

This thesis is the cross-cutting architectural decision that ties every
milestone together. Per-milestone tactical decisions live in milestone
files; this is the one written across the whole roadmap.

## Questions

Each question is at roadmap altitude — cross-cutting choices that affect
multiple milestones. Per-milestone detail is settled in milestone files (or
in their `/plan` follow-ups).

### Q1 (suggested): roadmap name `lp-shader-aggregates`?

Dir already exists at `docs/roadmaps/2026-04-22-lp-shader-aggregates/`.
Plan basenames will be `lp-shader-aggregates-m<N>-<slug>`.

**Suggested answer:** Yes.

### Q2 (suggested): unified pass-by-pointer ABI for *all* aggregates (arrays + structs + arrays-of-structs)?

This is the architectural thesis above. Confirms the scope of the array
migration (it changes the existing array ABI, breaking nothing
behaviourally but rewriting the calling-convention shape).

**Suggested answer:** Yes.

### Q3 (suggested): aggregate returns are always `sret` (caller-allocated dest slot, hidden first pointer arg)?

The alternative is keeping flat-scalar returns for small aggregates and
switching to `sret` only for big ones. The "always-`sret`" path is
uniform with the rest of the aggregate ABI; the "small fast path" trades
some uniformity for register-return performance on tiny structs (`Point`
returns of 2 floats). Since `lpvm-native`'s sret threshold logic already
picks `sret` automatically for structs that exceed the return-register
budget, the only difference is the small-struct case.

**Suggested answer:** Always `sret`. Uniformity wins. The small-struct
register-return optimisation is exactly the kind of follow-up perf work
that the optimisation milestone (or a future roadmap) covers, and we
don't want to bake it into the foundational ABI.

### Q4 (suggested): read-only-`in` optimisation as a *separate, later* milestone?

Skipping the entry-`Memcpy` for `in` aggregates the callee never writes
is a static-analysis pass over the callee body — it's cleanly separable
from the foundational ABI. Doing it as a separate milestone:

- Lets the foundational ABI land first, simple and obviously correct.
- Means tiny `in Point p` calls pay a temporary `Memcpy`-on-entry cost
  during the intermediate milestones.
- Avoids coupling backend correctness to optimisation correctness.

**Suggested answer:** Yes — separate milestone, after struct + array-of-
struct support land. The intermediate perf cost is acceptable; we'll be
running synthetic benchmarks in domain work, not shipping production
load yet.

### Q5 (suggested): array migration is a breaking change to the existing array calling convention. Acceptable?

The existing array `in` ABI passes flat scalars; migrating to pointer ABI
changes every call site that takes an array argument. All existing tests
must be re-validated. No GLSL semantic change — runtime results stay
identical — but the LPIR shape differs, which means any tooling that
inspects pre-link IR (filetest checks, debug printers, IR dumps) needs
updating.

**Suggested answer:** Yes — this is exactly the right time. Doing the
migration alongside struct introduction means we never have to maintain
two aggregate ABIs in parallel. Existing array filetests will tell us
loudly if something regresses.

### Q6 (suggested): milestone shape — foundation-first?

Three plausible orderings:

- **Foundation-first**: M1 establishes the pointer-ABI infrastructure
  *and* migrates arrays. M2 layers struct support on top. M3 layers
  arrays-of-structs. M4 adds uniform-with-array-field. M5 adds the
  read-only optimisation. M6 cleanup.
- **Big-bang aggregate ABI**: M1 introduces pointer ABI for arrays AND
  structs simultaneously. Higher review cost but no intermediate
  asymmetry.
- **Struct-first**: M1 adds structs with new pointer ABI; arrays stay
  flat-scalar (asymmetric world). M2 migrates arrays. Rejected — user
  explicitly said "align array with whatever we do."

**Suggested answer:** Foundation-first. Sketched milestones:

- **M1: Pointer-ABI foundation + array migration.** Introduce the new
  ABI. Migrate existing array `in` params from flat-scalar to pointer.
  All existing array tests stay green. No new GLSL features.
- **M2: Struct lowering on the unified ABI.** All struct filetests pass
  on `jit.q32` and `wasm.q32`.
- **M3: Arrays of structs (locals + params).** Extends `ArrayInfo` leaf
  to allow `LpsType::Struct`; the const-array-size filetest unblocks.
- **M4: Uniform-struct-with-array-field.** Extends
  `load_lps_value_from_vmctx` for array members. Small, isolated.
- **M5: Read-only-`in` optimisation.** Skip-Memcpy pass; measurable
  perf delta on tiny aggregates.
- **M6: Cleanup, RV32 backend validation, filetest sweep, docs.**

### Q7 (suggested): RV32 backend ABI is per-milestone or one final pass?

Each milestone produces some calling-convention change that *should* fall
out of cranelift's RV32 codegen automatically (since cranelift handles
the calling convention from a flat type signature). But "should" needs
validation — and validation is best done as part of each milestone's
acceptance, rather than batched into a final cleanup phase where
regressions are hard to bisect.

**Suggested answer:** Per-milestone. Each milestone enables its own
`@unimplemented(rv32c.q32)` / `@unimplemented(rv32n.q32)` markers as
part of its acceptance gate. M6 is just a "find anything we missed"
sweep, not the primary RV32 validation point.

## Notes

### Answered

- **Q1**: Yes. Roadmap name `lp-shader-aggregates`. Dir
  `docs/roadmaps/2026-04-22-lp-shader-aggregates/`. Plan basenames
  `lp-shader-aggregates-m<N>-<slug>`.

- **Q2: Unified pass-by-pointer ABI for all aggregates.** Yes.
  - Arrays migrate from flat-scalar-by-value to pointer-to-slot. No GLSL
    semantic change; runtime values stay identical. No real existing use
    of array params/returns in user code, so the call-site behaviour
    change is contained to the test corpus.
  - Structs land directly on the unified ABI from day one.
  - Arrays-of-structs slot in naturally.
  - Returns are always `sret`-style (caller-allocated dest slot, hidden
    first pointer arg).
  - Tiny-aggregate perf hit accepted short-term; recovered by the
    read-only-`in` optimisation (Q4); small-struct-register-return is
    follow-up perf work.

  **Important consequence — host ABI is trivial.**
  `lp-shader/lpvm/src/lpvm_data_q32.rs::LpvmDataQ32` is already a
  typed, std430-laid-out `Vec<u8>` constructed from any `LpsType` via
  `from_value(ty, &LpsValueF32)`, with `as_ptr()` / `as_mut_ptr()`. Once
  the JIT call ABI is "pass pointer to std430 buffer", the rust caller
  flow is:

  ```rust
  // in arg
  let arg = LpvmDataQ32::from_value(arg_ty, &value)?;
  // pass arg.as_ptr()

  // sret return
  let mut ret = LpvmDataQ32::new(return_ty);
  // pass ret.as_mut_ptr() as hidden first arg
  // call ...
  let result = ret.to_value()?;
  ```

  No flatten/unflatten on the host side at all. The struct/array
  cases in `lpvm_abi::flatten_q32_arg` and `decode_q32_return` largely
  *go away* — they're replaced by `LpvmDataQ32` construction. Scalars
  / vec / mat keep their existing flat-scalar marshalling.

- **Q3: Aggregate returns are always `sret`.** Yes (subsumed by Q2 — the
  uniform aggregate ABI applies to returns too). Caller-allocated dest
  slot, hidden first pointer arg, callee writes through it. Small-struct
  register-return optimisation is explicit follow-up perf work.

- **Q4: Read-only-`in` optimisation is M5 (separate, later).** Yes.
  Static analysis pass over the callee body — if the param's pointer is
  never the destination of a `Statement::Store` and never passed onward
  as an `inout`/`out` arg, elide the callee-side slot allocation and
  entry-Memcpy; let the callee Load directly from the caller's slot.
  Cleanly separable from the foundational ABI; lands after struct +
  arrays-of-structs.

- **Q5: Array migration is a behaviour change, M1 acceptance is "all
  existing array tests still green."** Yes. No real existing use of
  array params/return values in user code, so the call-site change is
  contained to the test corpus. Filetests are the safety net; if any
  regress we find out in M1.

- **Q6: Milestone shape — foundation-first.** Yes.
  - **M1**: Pointer-ABI foundation + array migration. New `LowerCtx`
    plumbing for aggregate slot allocation + pointer args; new
    `lower_call` shape (`Address(slot)` + `Memcpy`-on-entry); host
    ABI marshalling via `LpvmDataQ32::as_ptr`/`as_mut_ptr`; existing
    array tests stay green.
  - **M2**: Struct lowering on the unified ABI. `naga_type_to_ir_types`
    extension; `StructInfo` + `struct_map`; `lower_struct.rs`;
    `AccessIndex`/`Compose`/`Store` extensions. All
    `struct/`+`function/{param,return}-struct.glsl` filetests pass on
    `jit.q32` and `wasm.q32`.
  - **M3**: Arrays of structs (locals + params). `ArrayInfo` leaf
    extends to `LpsType::Struct`; element stride = std430-aligned
    struct size; `const/array-size/struct-field.glsl` unblocks.
  - **M4**: Uniform-struct-with-array-field. Extend
    `load_lps_value_from_vmctx` to recurse through array members at
    std430-strided offsets. Small, isolated.
  - **M5**: Read-only-`in` optimisation.
  - **M6**: Cleanup, full RV32 sweep, filetest re-enablement
    verification, docs.

- **Q7: RV32 backend validated per milestone.** Yes. Each milestone
  enables its own `@unimplemented(rv32c.q32)` / `@unimplemented(rv32n.q32)`
  markers as part of its acceptance gate. M6 is a "find anything we
  missed" sweep, not the primary RV32 validation point.
