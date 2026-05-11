# M3 — Arrays of Structs: Plan

## Notes

### Scope of work

Enable GLSL arrays of structs as local variables and function parameters:
- `Point ps[8];` declaration and zero / initializer-list initialization
- Element member access: `ps[i].x`, `ps[0].position.y`
- Element assignment: `ps[i] = Point(1.0, 2.0);` and member assignment `ps[i].x = 5.0;`
- Function parameters: `void foo(inout Point ps[4])` (and `out`)
- Outer-struct-with-array-of-struct field: `s.ps[i].x` (M2 already handles `s.ps`
  as a struct field; M3 extends through the array index).

**Out of scope:**
- Uniform blocks with array-of-struct (M4)
- Read-only `in` optimization (M5)
- Nested arrays of arrays of structs (`Point ps[4][4]`) — defer unless it falls
  out for free
- Array-of-struct equality (`ps == qs`) — defer with TODO + filed bug unless
  trivially supported (see Q6)

### Current state

**M1 (Arrays) is complete:**
- `AggregateInfo` in `lower_ctx.rs` tracks array locals/params with
  `leaf_element_ty`, `leaf_stride`, `element_count`
- `lower_array.rs` has element load/store via flat index × stride, with the
  address math inlined in four places (`load_array_element_const`,
  `load_array_element_dynamic`, `store_array_element_const`,
  `store_array_element_dynamic`)
- `lower_array_multidim.rs` handles multidimensional arrays

**M2 (Structs) is complete:**
- `AggregateInfo` with `AggregateLayout` (and `AggregateKind::Struct { members }`)
  handles struct locals/params
- `lower_struct.rs` provides member load/store, Memcpy, compose-into-slot,
  plus `peel_struct_access_index_chain_to_global` for global struct chains
- `naga_util::aggregate_layout` is the single source of truth for type-level
  layout decisions
- `store_lps_value_into_slot` (`lower_aggregate_write.rs`) recursively writes
  `LpsType` values into memory slots

**The gap (arrays-of-structs):**

Concretely, when `leaf_element_ty` would resolve to `LpsType::Struct`:

1. `flatten_local_array_shape` rejects struct leaves (only scalar/vector/matrix)
2. `aggregate_layout`'s `Array` arm uses `flatten_array_type_shape` which
   inherits the same restriction
3. Element load/store helpers in `lower_array.rs` assume the leaf is flat
   (scalar/vector/matrix → list of `IrType` → emit per-component Load/Store)
4. `AccessIndex` chain `ps[i].x` has no handler that combines the array index
   step with the struct member step
5. `store_lps_value_into_slot` for a struct leaf needs the struct's
   `AggregateLayout` (member offsets), not just the `LpsType` — this is what
   the M3 exploratory test hit:
   `internal lowering error: store_lps_value_into_slot: struct needs AggregateLayout with members`
6. `lower_array_equality_vec` currently dispatches per leaf inner type and only
   handles Scalar/Vector/Matrix — Struct leaf would fail today

**Existing test scaffolding:**
- `struct/array-of-struct.glsl` exists with 19 cases, marked `// test error`
  (compile-fail) — flips to `// test run` once M3 lands
- `const/array-size/struct-field.glsl` has `@unimplemented` markers to clear

### Resolved questions

| ID  | Question                                                                                                                                  | Resolution |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| Q1  | Should `leaf_stride` for a struct-leaf array be `lps_shared::layout::array_stride(&LpsType::Struct{...})`?                                 | **Yes.** No new code: `array_element_stride` already calls `array_stride(&lps, ...)`; works for `LpsType::Struct` once `flatten_*_array_shape` permits struct leaves. |
| Q2  | Extract `array_element_address(ctx, info, idx) -> VReg` and refactor the four existing inlined call sites in `lower_array.rs` to use it?  | **Yes.** Single primitive used by const-idx, dynamic-idx, scalar-leaf, and struct-leaf paths. |
| Q3  | Routing for `ps[i].x`, `ps[i].pos.x`, `s.ps[i].x`?                                                                                        | **Yes — peeler** `peel_arrayofstruct_chain` analogous to M2's `peel_struct_access_index_chain_to_global`, shared by `lower_expr` (Load) and `lower_stmt` (Store). |
| Q4  | New tests in `lps-filetests/filetests/array/of-struct/`?                                                                                  | **Yes.** Existing `struct/array-of-struct.glsl` flips to `// test run` as the broad smoke test; per-shape cases live under the new subdir. |
| Q5  | How to thread `AggregateLayout` to `store_lps_value_into_slot` for struct leaves?                                                          | **Investigation done.** The function already accepts `Option<&AggregateLayout>`; the failing call site in `lower_array_initializer` (`lower_array.rs:413`) passes `None`. Fix is ~5 lines: when `leaf_lps` is `Struct`, resolve `aggregate_layout(module, leaf_naga)?` once and pass `Some(&layout)`. |
| Q6  | Defer array-of-struct equality?                                                                                                            | **Yes.** `lower_array_equality_vec` errors clearly today; M3 keeps the eq test gated behind `// @unimplemented` (or a `// TODO(bug-N)` with filed follow-up) and does not implement it. |

## Design

### Surface area (smaller than initially feared)

After reading the actual code, the M3 surface is narrower than the roadmap
suggests, because the M2 primitives generalize naturally:

- `array_element_stride` already accepts any `LpsType` leaf (including
  `Struct`); std430 stride is via `lps_shared::array_stride`.
- `AggregateInfo::{element_count, leaf_stride, total_size, leaf_element_ty}`
  are handle-based and don't care if the leaf is a struct.
- `aggregate_layout` for the **leaf struct type** gives us member offsets via
  `AggregateLayout::struct_members()`.
- `store_lps_value_into_slot` already handles `LpsType::Struct` end-to-end —
  it just needs `Some(&layout)` from the array call sites.

### Components to add or change

**1. Permit struct leaves in array shape walks** (`lower_array_multidim.rs`)

`flatten_local_array_shape` and `flatten_array_type_shape` currently break out
of the loop on any non-`Array` `TypeInner` and call it the leaf. Today the
caller chain implicitly assumes scalar/vector/matrix; with M3 it assumes
"any non-array". The walks already work — no code change beyond removing or
relaxing any downstream assertions that reject `TypeInner::Struct` leaves.
(Audit: `naga_type_to_ir_types`, `array_element_stride`, `naga_to_lps_type`
all handle `Struct` post-M2.)

**2. New address helper** (`lower_array.rs`)

```text
pub(crate) fn array_element_address(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index: ElementIndex,        // Const(u32) | Dynamic(VReg)
) -> Result<VReg, LowerError>
```

Behaviour: clamp dynamic index, multiply by `info.leaf_stride()`, add to
`aggregate_storage_base_vreg`. For `Const`, fold the offset and return
`base + const_off` (just `Iadd` with an `IconstI32`, mirroring current
inlined math). Refactor the four existing const/dynamic load/store helpers
to call this (they then do per-IR-type Load/Store at offset 0 from the
returned VReg, except the const path can keep using `base + immediate offset`
form to avoid an extra add — see "Optimization notes" below).

**3. Struct-leaf load/store helpers** (`lower_struct.rs` — extend M2 module)

```text
pub(crate) fn load_array_struct_element(
    ctx, info, index,
    member_chain: &[u32],   // empty = whole struct (used for Compose ctx)
) -> Result<VRegVec, LowerError>

pub(crate) fn store_array_struct_element(
    ctx, info, index,
    member_chain: &[u32],
    rhs_expr: Handle<Expression>,
) -> Result<(), LowerError>
```

Both call `array_element_address` to get the element base, then walk
`member_chain` against `info`'s leaf-struct `AggregateLayout` (cached or
recomputed) to sum a constant member offset, then dispatch:
- **Load + non-empty chain → leaf scalar/vector/matrix**: per-IR-type Load
- **Load + empty chain → whole struct value**: not needed in M3 (whole-struct
  rvalue from array element is uncommon; if it appears, fall through to error
  and we'll add it as a follow-up — Naga typically only consumes member values)
- **Store + non-empty chain**: per-IR-type Store
- **Store + empty chain (whole-element)**: try memcpy fast path, else
  `store_lps_value_into_slot(..., Some(&leaf_layout))`

**4. Chain peeler** (`lower_struct.rs`)

```text
pub(crate) struct ArrayOfStructChain {
    pub array_info: AggregateInfo,        // resolved aggregate (the array)
    pub index: ElementIndex,              // Const(u32) | Dynamic(Handle<Expression>)
    pub member_chain: SmallVec<[u32; 4]>, // struct AccessIndex steps after the array
    pub leaf_layout: AggregateLayout,     // layout of the struct leaf
}

pub(crate) fn peel_arrayofstruct_chain(
    ctx: &LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Option<ArrayOfStructChain>
```

Walk `expr` upward through `AccessIndex` (struct member) and exactly one
`Access` or `AccessIndex` (array index), stopping when we either:
- Reach `LocalVariable` whose `aggregate_map` entry has a struct leaf, **or**
- Reach a chain `AccessIndex { base: <struct-local-loaded>, index: field_idx }`
  where field_idx names an array-of-struct field on the outer struct (the
  `s.ps[i].x` case — Naga shape is array `Access` whose base is a struct
  `AccessIndex`). For M3 we support this case **only** when the outer struct
  is a slot-backed local; outer-struct-field-of-array-of-struct on globals is
  M4 work and emits `UnsupportedExpression`.

If the array root is `s.ps...`, the address resolution adds the constant
member offset for `ps` to the slot base before the index multiply.

**5. Wire the peeler into `lower_expr` and `lower_stmt`**

- `lower_expr::AccessIndex`: before falling through to existing arms, try
  `peel_arrayofstruct_chain` on the full expression. On hit, dispatch to
  `load_array_struct_element` and cache the resulting `VRegVec`.
- `lower_stmt::Store`: same pattern on the LHS pointer, dispatch to
  `store_array_struct_element` with the RHS expression handle.

**6. Initialization paths** (`lower_array.rs`)

- `lower_array_initializer`: when `leaf_lps` is `Struct`, compute
  `leaf_layout = aggregate_layout(ctx.module, leaf_naga)?` once, then pass
  `Some(&leaf_layout)` to each `store_lps_value_into_slot` call (currently
  passes `None` at line 413). Same change in the tail-zero loop — actually
  `zero_leaf_lps_in_slot` already routes Struct to `zero_struct_at_offset`,
  so that path is fine.
- `zero_fill_array_slot` (`lower_array.rs:182`): currently iterates
  per-IR-type Stores assuming flat leaf. For struct leaves, replace inner
  loop with `zero_struct_at_offset(fb, base, byte_off, leaf_naga)` (need to
  add a `&FunctionBuilder` variant of `zero_struct_at_offset` since current
  one takes `&mut LowerCtx`; or thread `LowerCtx` here — `lower_ctx::new`
  already has `&mut LowerCtx` in scope at zero-fill time, so probably trivial).

  *Verify in implementation*: zero_fill is called from `LowerCtx::new` which
  doesn't have a built `LowerCtx` yet — that's why it takes `FunctionBuilder`
  directly. Resolution: either (a) add a `&FunctionBuilder` variant of
  `zero_struct_at_offset` (likely small), or (b) defer struct zero-fill to
  the first explicit reference and rely on initializer/store paths.
  Recommended: (a) — keep zero-init semantics simple.

**7. Param ABI** (`lower_call.rs`, `lower_ctx.rs`)

`out`/`inout T[N]` already passes a pointer (M1). For `out`/`inout Point[N]`
no ABI change is needed — the callee's `aggregate_map` entry now resolves
the leaf to `LpsType::Struct` and all M3 helpers above kick in.

**8. Filetests**

Flip `struct/array-of-struct.glsl` from `// test error` → `// test run` and
verify it passes. Add focused per-shape tests under
`lps-filetests/filetests/array/of-struct/`:

- `local-basic.glsl` — `Point ps[4]`, member read/write, dynamic index
- `local-init-list.glsl` — `Point ps[3] = Point[3](Point(...), ...)`
- `inout-param.glsl` — callee mutates `inout Point ps[N]`
- `nested-field.glsl` — outer struct contains array-of-struct field
- `zero-init.glsl` — `Point ps[N];` (default zero) followed by reads
- `whole-element-store.glsl` — `ps[i] = q;` (memcpy path)

Toggle `@unimplemented` off on `const/array-size/struct-field.glsl` and confirm.

Out of M3 (file follow-up bugs):
- `eq.glsl` — `ps == qs` (array-of-struct equality)
- `multidim.glsl` — `Point ps[4][4]` (only if it falls out for free)

### Module boundaries

| Concern | Module |
|---------|--------|
| Address math (`array_element_address`) | `lower_array.rs` |
| Struct-leaf element load/store | `lower_struct.rs` (extends M2 module) |
| Chain peeler | `lower_struct.rs` |
| Array shape walk relaxation | `lower_array_multidim.rs` |
| Initializer / zero-fill threading | `lower_array.rs` |
| `AccessIndex` / `Store` dispatch | `lower_expr.rs`, `lower_stmt.rs` |
| Layout (already done) | `naga_util.rs`, `lower_aggregate_layout.rs` |

No new modules. Roughly +250 LOC frontend, –40 LOC from address-math
deduplication, ~6 new filetests.

### Phase breakdown (suggested)

The work splits cleanly into three phases. Each phase commits independently;
intermediate states are intentionally allowed to leave some array-of-struct
tests still failing per the M2-style "intentional partial state" policy.

- **Phase 1 — Plumbing.** Allow struct leaves in `flatten_*_array_shape`,
  add `array_element_address` and refactor existing array load/store call
  sites to use it (no behaviour change for scalar leaves), thread
  `Some(&layout)` in `lower_array_initializer` for struct leaves, and add a
  `&FunctionBuilder` variant of `zero_struct_at_offset` for `zero_fill_array_slot`.
  *Acceptance*: simple `Point ps[4];` with no access compiles (declaration +
  zero-init only). Member access still fails.
- **Phase 2 — Element access.** Add `load_array_struct_element` /
  `store_array_struct_element` and `peel_arrayofstruct_chain`; wire into
  `AccessIndex` (Load) and `Statement::Store`. Whole-element memcpy path.
  *Acceptance*: `ps[i].x`, `ps[i].pos.x`, `ps[i] = q` all work for
  slot-backed locals.
- **Phase 3 — Outer-field-of-array-of-struct + tests.** Extend the peeler
  for `s.ps[i].x` (struct-member-of-local containing array-of-struct).
  Flip `struct/array-of-struct.glsl` to `// test run`, add the new
  `array/of-struct/` corpus, clear `@unimplemented` on
  `const/array-size/struct-field.glsl`. File deferred-eq follow-up bug.
  *Acceptance*: full corpus passes on `wasm.q32`, `rv32c.q32`, `rv32n.q32`
  (parity per M2 policy). `jit.q32` is informational, not gating.

### Risks / open implementation notes

- **`zero_struct_at_offset` requires `&mut LowerCtx`** but `zero_fill_array_slot`
  runs from `LowerCtx::new` and only has `&mut FunctionBuilder`. Verify in
  Phase 1; if extracting a `FunctionBuilder`-only variant is invasive, fall
  back to lazy zero-on-first-write or make `zero_fill_array_slot` an
  after-`new` step in `LowerCtx::new`'s flow.
- **`load_array_struct_element` with empty `member_chain`**: i.e. extracting
  a whole struct rvalue from `ps[i]` for use as a function argument. Naga may
  or may not generate this; if not encountered in the test corpus, leave a
  clear `UnsupportedExpression` error and add to follow-ups.
- **Optimization (deferred)**: Const-index path could fold the offset into
  the LpirOp `offset` field rather than emitting `Iadd base, IconstI32`.
  Phase 1 keeps it simple; revisit only if codegen size becomes a concern.

### Acceptance criteria (M3)

Per the M2-style policy:

- All M3 filetests pass on `wasm.q32`, `rv32c.q32`, `rv32n.q32`.
- `rv32c.q32` and `rv32n.q32` have parity (same set of passing tests).
- Any failure that *should* pass per M3 scope is fixed within M3, not deferred.
- Anything explicitly out of scope (eq, multidim, M4 globals) is marked
  `// @unimplemented(...)` or `// TODO(bug-N)` with a filed follow-up.
- `jit.q32` is informational only.
