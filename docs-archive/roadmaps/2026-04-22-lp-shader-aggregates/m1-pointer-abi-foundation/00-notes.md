# M1 — Pointer-ABI foundation + array migration · Notes

Roadmap: `docs/roadmaps/2026-04-22-lp-shader-aggregates/`
Milestone file: `m1-pointer-abi-foundation.md`

## Scope of work

Establish the unified pass-by-pointer aggregate ABI end-to-end and migrate the
existing array `in` calling convention from "flat scalars by value" to
"pointer-to-slot + Memcpy-on-entry". After M1, the codebase has exactly one
aggregate ABI; M2+ layer struct support, arrays-of-structs, and uniform-array
fields onto it without touching the calling convention again.

Surface area:

- **Frontend (`lp-shader/lps-frontend`).** Generalise `ArrayInfo` /
  `ArraySlot` into an `AggregateSlot` shape that's struct-ready for M2.
  Rewrite the `in T[]` parameter path: replace many flat scalar VReg params
  with one `Pointer` param, and emit `Memcpy(slot_addr, param_addr, size)`
  at function entry. Rewrite the call path: aggregate args become "address
  of caller's slot", aggregate returns become "caller-allocated dest slot,
  hidden first `sret` arg". `inout`/`out` array path is already
  pointer-based and stays as-is.
- **LPIR (`lp-shader/lpir`).** No new ops. Introduce one piece of metadata
  on `IrFunction` that records "this function returns its result via an sret
  pointer arg" (see Q3). All four backends consult it.
- **Host ABI marshalling (`lp-shader/lpvm/src/lpvm_abi.rs`).** Aggregate
  arms of `flatten_q32_arg` / `decode_q32_return` collapse to
  `LpvmDataQ32::from_value(...).as_ptr()` / pre-allocated dest buffer +
  `to_value()`. Scalar / vec / mat marshalling is unchanged.
- **Backends.** All four codegen paths consume the new "one pointer per
  aggregate arg, one optional sret hidden first arg" shape:
  - `lpvm-cranelift` (host JIT + RV32 cross): mostly already supports sret;
    needs to switch from "trigger sret on scalar return count > N" to
    "trigger sret when frontend says so".
  - `lpvm-native` (from-scratch RV32 backend): update `scalar_count_of_type`
    consumers; arg classifier becomes "1 word per aggregate pointer".
  - `lpvm-wasm` (wasmtime + browser): the trickiest backend (Q5). Pointers
    in wasm linear memory require host-side buffer allocation in linear
    memory at every call boundary.
  - `lpvm-emu` (software emulator, used for tests): not mentioned in the
    milestone; needs decision (Q4).
- **Filetests.** Bulk update `CHECK:` lines for any test that asserts pre-link
  IR shape involving an array `in` arg ("flat scalar args" → "one pointer
  arg + entry Memcpy"). Add at least one new filetest exercising aggregate
  (array) `sret` return round-tripped end-to-end on every backend.

Out of scope (deferred milestones):

- Struct support (M2).
- Arrays of structs (M3).
- Uniform-with-array-field (M4).
- Read-only-`in` optimisation (M5).
- Small-aggregate register-return fast path (perf follow-up roadmap).

## Current state of the codebase

### Frontend — what exists today

- **`LowerCtx::new`** (`lower_ctx.rs:91`) classifies each argument three ways:
  - `TypeInner::Pointer { space: Function }`: one `IrType::Pointer` param;
    base `Handle<Type>` recorded in `pointer_args`. Used today by
    `inout`/`out` of arrays (and any other type).
  - `TypeInner::Array { .. }`: many flat scalar params via
    `array_ty_flat_ir_types`. **This is the migration target.**
  - everything else: `naga_type_to_ir_types` (scalar/vec/mat). Stays
    unchanged.
- **`ArrayInfo`** (`lower_ctx.rs:51`) holds `slot: ArraySlot::Local(SlotId) |
  Param(arg_i)`, plus dimensions / leaf type / stride / element_count.
  Already slot-backed for both locals and `inout`/`out` params. The shape
  is already what M1 wants — needs renaming/generalising into `AggregateSlot`
  so M2 can plug structs into the same machinery.
- **`scan_param_argument_indices`** (`lower_ctx.rs:357`) — walks the body
  for `Store(LocalVariable, FunctionArgument)` and records aliases. Excludes
  `is_array_val` (the Naga `Array` arg type) so array `in` locals are
  slot-backed and the entry Store goes through
  `lower_array::store_array_from_flat_vregs` instead. With the new ABI this
  filter expands to "all aggregates" (M2 will add struct, M3
  arrays-of-structs).
- **`lower_array::store_array_from_flat_vregs`** (`lower_array.rs:592`) —
  callee prologue path that copies many flat-scalar args into the local
  slot one Store at a time. **This whole function disappears in M1.**
  Replaced by a single `Memcpy(local_slot_addr, param_pointer, size)`.
- **`lower_array::load_array_flat_vregs_for_call`** (`lower_array.rs:627`) —
  caller path that loads every scalar from the caller's slot into VRegs
  before pushing them as call args. **Also disappears in M1.** Replaced by
  `arg_vs.push(slot_address_vreg)`.
- **`lower_user_call`** (`lower_stmt.rs:504`) — splits args into:
  - `TypeInner::Pointer` arm (line 554): for arrays already in a slot,
    pushes the slot address. For non-array pointer types, allocates a temp
    slot, stores the local's flat VRegs into it, pushes its address, and
    schedules a copy-back loop after the call. **The "non-array pointer"
    half of this stays for scalar `inout`/`out` (which doesn't exist in
    GLSL but is what Naga does); the array half is the model for the new
    universal aggregate path.**
  - `TypeInner::Array` arm (line 577): the flat-scalar aggregate path.
    **Migration target.** Becomes "push slot address, exactly like the
    array-pointer arm above."
  - default: scalars/vec/mat — unchanged.
- **Aggregate return** in `lower_user_call` (line 595–615): allocates one
  result VReg per scalar return component and threads them through
  `push_call`. **Migration target.** Becomes "alloc a dest slot, push its
  address as the hidden first arg, post-call read individual scalars from
  the slot to materialise the `CallResult` VRegs." (Or: keep the result
  VRegs as-is and have the backend handle the "load from slot" for us; see
  Q3 — this is what `lpvm-cranelift` already does.)
- **`Statement::Return`** (`lower_stmt.rs:95`): today emits
  `push_return(&vs)` with one VReg per scalar return component. With sret
  for aggregate returns, becomes `Memcpy(sret_arg_vreg, value_slot_addr,
  size)` followed by `push_return(&[])`. Or, equivalently, "Store each
  scalar component into the sret buffer, then return nothing" — which is
  exactly what `lpvm-cranelift::emit/call.rs` already does for its existing
  RV32-many-returns sret path (line 166–180). The frontend can keep
  emitting per-scalar returns and let the backend do the sret transform —
  see Q3.

### Naga signature helpers

- **`naga_type_to_ir_types`** (`naga_util.rs:36`) — errors on
  `TypeInner::Struct` and `TypeInner::Array`. Stays as the
  scalar/vec/mat-only helper. M2 will add a separate path for structs that
  returns "one `IrType::Pointer` per struct param" rather than flattening.
- **`array_type_flat_ir_types`** (`naga_util.rs:109`) — returns one
  `IrType` per scalar component of an array. **Replaced** by a helper
  returning a single `IrType::Pointer` for the param (call it
  `array_ty_pointer_arg_ir_type`). Old name stays only if some non-call
  consumer wants the flat shape (audit needed).
- **`func_return_ir_types`** (`naga_util.rs:132`) — array returns flatten
  to many scalar IR types. **Migration target:** wrap into
  `func_return_ir_types_with_sret` that returns
  `(returns: Vec<IrType>, sret_arg: Option<IrType::Pointer>)`. Aggregate
  returns produce `(vec![], Some(IrType::Pointer))`; scalar/vec/mat
  produce `(scalar_types, None)`.
- **`ir_types_for_naga_type`** (`naga_util.rs:97`) — used outside the
  param/return signature path; needs audit to confirm none of its callers
  are call-site-specific (and if they are, they need migrating too).

### LPIR — what's already in place

- `LpirOp::Memcpy { dst_addr, src_addr, size }` exists
  (`lpir_op.rs:392`). No new op needed.
- `LpirOp::SlotAddr { dst, slot }` produces an `IrType::Pointer` VReg from
  a `SlotId`. Already used everywhere.
- `IrType::Pointer` exists and is honored by all backends
  (`lpir/types.rs:20`).
- `IrFunction { name, vmctx_vreg, param_count, return_types, vreg_types,
  slots, body, vreg_pool }` (`lpir_module.rs:37`). **No sret marker
  today.** sret is currently *backend-derived* by `lpvm-cranelift` from a
  return-scalar-count heuristic (see below).
- `FunctionBuilder::add_param(IrType)` adds a normal user param; there is
  no "add hidden sret param" path yet.

### Host ABI marshalling

- **`lpvm_abi::flatten_q32_arg`** (`lpvm_abi.rs:133`) — handles
  scalar/vec/mat/`Array { ... }` (recursive), errors on `Struct`. Today
  the Array arm emits one `i32` word per scalar component, matching the
  flat-scalar wire ABI. **Migration target.** New shape: compute scalar
  components for scalar/vec/mat as today; for Array (and in M2 for
  Struct), build an `LpvmDataQ32::from_value(ty, value)` and pass its
  pointer as a single arg.
- **`decode_q32_return`** (`lpvm_abi.rs:275`) — same story for returns.
  New shape: caller pre-allocates `LpvmDataQ32::new(ret_ty)`, passes
  `as_mut_ptr()` as the hidden first arg, then `to_value()` after the
  call.
- **`LpvmDataQ32`** (`lpvm/src/lpvm_data_q32.rs:14`) is the host's
  std430-laid-out byte-buffer wrapper, with `from_value`, `to_value`,
  `as_ptr`, `as_mut_ptr`. It already round-trips structs and arrays; the
  test at line 530 covers struct-with-vec3-and-float.
- The wire ABI for `lpvm-cranelift` is currently "Vec<i32>-of-flat-words";
  `LpvmDataQ32::as_ptr` returns `*const u8`. Reinterpreting a Vec<u8> with
  the same layout as the JIT's flat-i32 representation is sound on
  little-endian (Q32 lanes are stored as raw `i32` LE, std430 packs vec3
  as 3xi32 = 12B, mat3 as 9xi32 with 4B alignment of 12B columns = 36B,
  matching exactly). Need to verify std430 layout vs the existing flat-Q32
  layout for one or two corner cases (e.g. `Mat3` column padding) — see
  Q6.

### Backends

- **`lpvm-cranelift`** (`emit/mod.rs:88`):
  `signature_uses_struct_return(isa, func)` returns `true` when
  `RV32: return_types.len() > 2`, `host: > 4`. Today, mat3 (9 returns) and
  mat4 (16 returns) already trigger sret on RV32; on host, mat3 also (>4),
  mat4 also. So **the sret machinery exists and works** for the common
  cases — what changes in M1 is *who decides* (frontend, not the count
  heuristic) and *which types use it* (all aggregates regardless of size).
  - `signature_for_ir_func` (`emit/mod.rs:101`) places sret as `params[0]`,
    then vmctx, then user params. Same shape M1 wants.
  - `emit_call.rs::emit_call`: when calling a `callee_struct_return`
    function, allocates a stack slot, pushes its base as first arg,
    post-call reads individual scalars from the slot to materialise the
    LPIR result VRegs (line 50–87). **Same machinery the new ABI
    needs**, just triggered explicitly instead of by a count heuristic.
  - Test at `emit/mod.rs:308` confirms sret-first / vmctx-second /
    user-args-after ordering is verified.
- **`lpvm-native`** (RV32 from-scratch, `abi/classify.rs`):
  `scalar_count_of_type` recursively counts scalars; `entry_param_scalar_count`
  flattens param count. With the new ABI, aggregates contribute exactly
  **one** scalar (the pointer), not their flattened scalar count. This
  needs a parallel `entry_param_scalar_count_pointer_abi` or a flag on
  the existing helper.
  - The `ReturnMethod::Sret` enum variant exists (`abi/classify.rs:38`),
    with `ptr_reg`, `preserved_reg`, `word_count`. Today triggered by
    `func_abi_rv32` in `isa/rv32/abi.rs` based on return count. Same
    pattern as `lpvm-cranelift`: the trigger condition changes to
    "frontend says so", the rest of the machinery stays.
- **`lpvm-wasm`** (`rt_wasmtime/marshal.rs`): today flattens aggregates
  to many flat `Val::I32`s, both for args and returns. No notion of
  sret. **Pointer-ABI in wasm means pointers into the wasm instance's
  linear memory.** The host has to:
  - Allocate a region in linear memory for each aggregate arg, copy the
    data in via `memory.write`, pass the i32 offset.
  - Allocate a region for sret returns, pass the i32 offset, after the
    call read it back via `memory.read`.

  Wasm has a built-in linear-memory allocator (`memory.grow`) but no
  malloc-equivalent — we'd need a simple bump allocator exported from
  wasm or maintained host-side per call. The existing wasm path uses
  `LpvmDataQ32`-style buffers nowhere yet. **Q5 covers this.**
- **`lpvm-emu`** (software emulator, `lpvm-emu/src/`): used by some
  filetest backends. Not mentioned in M1's milestone scope. **Q4.**

### Existing array `in` filetests

- `lp-shader/lps-filetests/filetests/function/param-array.glsl` (the
  primary array-param test). 9 sub-tests covering `in`, `inout`, `out`,
  `const in`, vector-element arrays, bool arrays, multiple sizes. All
  must stay green through M1.
- Other tests with array params under `function/`, `lpvm/native/`, etc.
  — need a `rg` sweep to enumerate.
- CHECK-line tests asserting against pre-link IR shape (flat-scalar args)
  — need a `rg` sweep. R2 in the roadmap.

## Architectural thesis (recap from roadmap notes)

> All aggregates pass by pointer to a slot. Aggregate returns use a hidden
> first `sret` pointer arg (caller-allocated). Scalar / vec / mat unchanged.
> Local aggregates are always slot-backed.

## Questions

### Confirmation table

| #   | Question                                                                                  | Context                                                                                                                                                                  | Suggested answer                                                                                |
| --- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| Q1  | Plan name `lp-shader-aggregates-m1-pointer-abi-foundation`?                               | Matches roadmap slug + milestone basename verbatim.                                                                                                                      | Yes                                                                                              |
| Q2  | Generalise `ArrayInfo`/`ArraySlot` → `AggregateSlot { Local(SlotId), Param(arg_i) }`?      | M1 enum shape. Generic enough for M2 structs / M3 arrays-of-structs.                                                                                                     | Yes                                                                                              |
| Q7  | Extract call lowering into a new `lower_call.rs`?                                          | The call path grows substantially (per-aggregate slot/Memcpy plumbing, sret return materialisation). `lower_stmt.rs:504-633` is already sizable.                         | Yes — extract `lower_user_call` + helpers into `lower_call.rs` as part of M1.                   |
| Q8  | Manual filetest CHECK-line rewrites (no scripting)?                                       | Bulk change "flat scalar args" → "one pointer arg + entry Memcpy". Probably ~20–40 tests; manual is faster than writing a script.                                        | Yes — do it manually, one phase.                                                                |
| Q9  | Keep `Statement::Return` lowering emitting "per-scalar returns" and let the backend do the sret transform? | Frontend stays simple; backend already has the machinery. Alternative: frontend emits `Memcpy(sret_arg, dst_slot, sz) + push_return(&[])` and backend just sees an empty return. | Yes — keep frontend per-scalar; backend handles sret transform. (See Q3 for the marker.) |
| Q10 | Run a full filetest sweep (`jit.q32`, `wasm.q32`, `rv32c.q32`, `rv32n.q32`) at end of M1? | Per the milestone's Q7. RV32 backends specifically per R1 mitigation.                                                                                                    | Yes                                                                                              |

### Discussion-style questions

These need real iteration:

- **Q3** — `sret` representation in LPIR (what shape is the marker?)
- **Q4** — Does `lpvm-emu` need to track this milestone, or is it
  out-of-scope?
- **Q5** — `lpvm-wasm` pointer-ABI: where do aggregate buffers live in
  linear memory, and who allocates them?
- **Q6** — Is the std430 layout in `LpvmDataQ32` actually byte-identical
  to what the JIT-compiled shader code expects in memory for every type
  we care about?
- **Q11** — Phase decomposition / parallelism strategy.

(These get asked one at a time per `/plan` process, after Q1–Q2 / Q7–Q10
are confirmed.)

## Notes

### Resolved confirmations

- **Q1 ✓** Plan name: `lp-shader-aggregates-m1-pointer-abi-foundation`.
- **Q2 ✓** Generalise to `AggregateSlot { Local(SlotId), Param(arg_i) }`
  (and rename `ArrayInfo` → `AggregateInfo` accordingly).
- **Q7 ✓** Extract `lower_user_call` and helpers into a new
  `lower_call.rs` as part of M1.
- **Q8 ✓** Filetest CHECK-line rewrites done manually, in one phase.
- **Q9 ✓** Frontend keeps emitting per-scalar returns; backends do the
  sret transform (driven by the marker decided in Q3).
- **Q10 ✓** End of M1 runs the full filetest sweep across `jit.q32`,
  `wasm.q32`, `rv32c.q32`, `rv32n.q32`.

### Q3 — sret representation in LPIR

**Decision: Option A — explicit `sret_arg: Option<VReg>` field on
`IrFunction`.**

- New field `pub sret_arg: Option<VReg>` on `IrFunction`. When `Some(vreg)`:
  - `return_types` is empty.
  - `vreg_types[vreg.0 as usize] == IrType::Pointer`.
  - VReg numbering: `%0` = vmctx, `%1` = sret (when present), user params
    start at `%(1 + sret_offset)` where `sret_offset =
    sret_arg.is_some() as u32`.
  - `user_param_vreg(i)` updated accordingly; `total_param_slots()`
    accounts for the hidden sret slot.
- `ImportDecl` gets a parallel marker (e.g. `pub sret: bool`) so call
  sites can resolve sret-ness for both local and imported callees.
- Backends (`lpvm-cranelift`, `lpvm-native`, `lpvm-wasm`, `lpvm-emu`) read
  `func.sret_arg.is_some()` instead of running the count heuristic.
  `lpvm-cranelift::signature_uses_struct_return` collapses to a one-line
  field read.
- LPIR printer/parser learn a marker, e.g. `func @foo : sret %1 (vmctx
  %0, %2: f32, ...)`.
- Per Q9, frontend keeps emitting per-scalar `Return`s; the per-backend
  sret transform converts them into "store into sret buffer + ret void".
  (Or the frontend emits the `Memcpy` itself — concrete shape decided in
  the design phase.)

Rejected:

- Option B (convention only, no field) — can't distinguish sret-returning
  void from a real GLSL `void f(out S s)`.
- Option C (special `IrType::SretPointer` in `return_types`) — conflates
  "what is returned" with "how it's returned", no win over A.

### Q4 — `lpvm-emu` scope

**Decision: in scope (Option a) — full pointer-ABI equivalence.**

Rationale:

- `Backend::Rv32` is part of the filetest matrix; the M1 sweep (per Q10)
  needs all four backends green.
- It's the *easiest* of the three pointer-ABI backends (the other two
  being `lpvm-cranelift` and `lpvm-wasm`):
  - Callee codegen is free — `EmuModule::compile` reuses
    `lpvm-cranelift`. Once cranelift honours `IrFunction::sret_arg`,
    emu inherits it.
  - The emulator already has an sret execution path:
    `EmuInstance::run_emulator_call` (`instance.rs:502-510`) detects
    `ArgumentPurpose::StructReturn` and calls
    `call_function_with_struct_return(entry, args, sig, size_bytes)`.
    Only the size arg changes from `n_ret * 4` to
    `aggregate_size_bytes`.
  - Guest-memory allocation exists: `EmuSharedArena::alloc(size,
    align)`.
- Host marshalling: a single helper "given `LpvmDataQ32` + arena, allocate
  a guest buffer, copy bytes, return guest base as i32" handles both
  aggregate `in` args and the sret dest.

### Q5 — `lpvm-wasm` aggregate buffer placement

**Decision: Option A — host bumps the exported `$sp` shadow-stack
pointer.** Acts as a normal caller would: allocate temp space on the
shadow stack for each aggregate `in` arg and the sret dest, write/read
via `mem.write`/`mem.read`, restore `$sp` after the call (or rely on
`prepare_call` to reset on the next call).

Rationale:

- The shadow stack already exists (`SHADOW_STACK_BASE = 65536`,
  `FRAME_ALIGN = 16` in `emit/memory.rs`), is already exported via
  `SHADOW_STACK_GLOBAL_EXPORT`, and is reset to base by `prepare_call`
  before each call.
- Semantically aligned: aggregate args + sret returns are exactly
  per-call, stack-discipline allocations.
- Zero IR / codegen changes; only host marshalling changes
  (~50 LOC per runtime).
- Same mechanism for `rt_wasmtime` and `rt_browser` — only the
  wasmtime/JS API for `mem.write` and `global.set` differs.

Marshalling unification opportunity:

- The same shape works for `lpvm-emu` (allocates from `EmuSharedArena`)
  and `lpvm-wasm` (bumps `$sp`). The M1 marshal layer can abstract over
  a `(write_bytes, read_bytes, alloc_temp)` triple so the per-aggregate
  call dance is shared.

Rejected:

- Option B (export `__lp_alloc(size, align)` from each module) — needs
  new export per module + extra wasm `call` per aggregate per shader
  call.
- Option C (fixed host-managed scratch region below `$sp` base) —
  requires picking a size up front; conflicts with shadow-stack base.
- Option D (`Memory::grow` per call) — wasteful, slow.

### Q6 — single layout authority

**Decision: Option A — `lps_shared::layout::std430` is the single layout
authority for all aggregate memory in the project.**

Migration as part of M1:

- Frontend's array slot layout switches from "use Naga's `TypeInner::Array
  { stride }`" to "compute via `lps_shared::layout::array_stride(leaf_lps_type,
  LayoutRules::Std430)`" and `type_size(lps_ty, Std430)`.
  - Affects `flatten_local_array_shape` and `flatten_array_type_shape`
    in `lower_array_multidim.rs`.
  - Drops the `min_layout_stride = ir_components * 4` patch — std430
    handles bvec4 etc. natively.
  - Vec3 in arrays now strides by 12 (was potentially 16 if Naga gave
    16); existing array filetests may need re-baselining (folded into
    the manual filetest CHECK rewrite phase from Q8).
- A single utility (e.g. `lower_aggregate_layout::aggregate_size_and_align(
  naga_ty) -> (size, align)`) becomes the funnel for slot allocation,
  sret-arg sizing, host marshalling buffer sizing, and entry-Memcpy
  size.
- Debug assertion: `type_size(lps_ty, Std430) == frontend slot size`
  for every aggregate, so future drift is caught in CI.
- Result: `LpvmDataQ32` ↔ slot memory ↔ uniforms/globals ↔ vmctx all
  agree by construction. M2 (structs) and M3 (arrays-of-structs) plug
  in without revisiting layout.

Scope note: this adds a focused "migrate slot layout to std430" sub-phase
to M1 (1–2 frontend files, array filetest re-baseline, validate). Worth
it — it makes the user's "just hand the LpvmDataQ32 pointer" model true
by construction.

Rejected:

- Option B (host-side translation between std430 ↔ "wire layout") —
  introduces a third layout convention; per-call O(n) repack cost;
  undermines the value of `LpvmDataQ32`.
- Option C (empirically discover and patch as mismatches appear) —
  silent runtime bugs, repeated one-off fixes, no invariant that
  "future types just work."

### Q11 — phase decomposition

**Decision: 10 phases as proposed, with 4 parallel windows.**

```
P1.  LPIR sret marker                        [sub-agent: yes,  parallel: P2]
P2.  Layout authority migration (std430)     [sub-agent: yes,  parallel: P1]
P3.  Frontend aggregate + pointer ABI        [sub-agent: yes,  parallel: -]
P4.  lpvm-cranelift sret driven by marker    [sub-agent: yes,  parallel: P5, P7]
P5.  lpvm-native sret driven by marker       [sub-agent: yes,  parallel: P4, P7]
P6.  lpvm-emu host marshalling               [sub-agent: yes,  parallel: P7]
P7.  lpvm-wasm host marshalling              [sub-agent: yes,  parallel: P4, P5, P6]
P8.  lpvm_abi aggregate-arm cleanup          [sub-agent: yes,  parallel: P9]
P9.  Filetest CHECK rewrites + sret tests    [sub-agent: yes,  parallel: P8]
P10. Cleanup & validation                    [sub-agent: supervised]
```

Notes:

- P3 stays as one phase — boundaries are too tangled for a clean
  callee/caller split.
- Layout migration (P2) bundled into M1 — makes M2 (structs) much
  simpler.
- No P0 scaffolding phase — skipped to keep the plan tight.
