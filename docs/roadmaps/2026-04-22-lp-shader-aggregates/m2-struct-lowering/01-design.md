# M2 — Struct Lowering: Design

Settled questions: Q1–Q10 (see `00-notes.md`). This doc translates them
into the file-level architecture and module/function boundaries the
phases will implement.

## High-level shape

```
                         ┌───────────────────────────────────────────┐
                         │ naga_util.rs                              │
                         │   • naga_type_to_ir_types: + Struct arm  │
                         │     (std430-ordered flatten, used only   │
                         │      for value-coercion paths)           │
                         │   • expr_type_inner / expr_scalar_kind   │
                         │     + Struct AccessIndex arms            │
                         │   • aggregate_layout(module, ty)         │
                         │     → Option<AggregateLayout>            │
                         │       (Q8 — single source of truth)      │
                         │   • func_return_ir_types_with_sret:      │
                         │     consumes aggregate_layout            │
                         └─────────────┬─────────────────────────────┘
                                       │
                                       ▼
            ┌──────────────────────────────────────────────────────────┐
            │ lower_ctx.rs                                              │
            │   • AggregateInfo extended with AggregateKind:            │
            │       Array { dims, leaf_ty, leaf_stride, element_count } │
            │       Struct { members: Vec<MemberInfo> }                 │
            │   • LowerCtx::new — slot allocation switches to           │
            │     aggregate_layout(...) for both arrays + structs       │
            │   • By-value `in` struct param: same Memcpy-from-pointer  │
            │     entry shape as arrays (Q7)                            │
            │   • scan_param_argument_indices excludes struct value     │
            │     params from param_aliases (Q7)                        │
            └─────────────┬─────────────────────────────────────────────┘
                          │
                          ▼
   ┌──────────────────────────────────────────────────────────────────┐
   │ lower_aggregate_write.rs (NEW — Q9)                               │
   │   • store_lps_value_into_slot(ctx, base, offset, lps_ty, expr)   │
   │     – scalar/vector/matrix → typed Stores at offset               │
   │     – struct → recurse on members at member.offset                │
   │     – array  → recurse on elements at i*leaf_stride               │
   │     – memcpy fast path when expr is a slot-backed aggregate of    │
   │       matching type (whole-struct / whole-array assignment)       │
   │   • Used by lower_array initializer (refactor) AND lower_struct   │
   │     compose AND lower_call rvalue temp-slot materialisation       │
   └──────────────────────────────────────────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┬───────────────────┐
        ▼                 ▼                 ▼                   ▼
┌──────────────┐  ┌──────────────────┐  ┌──────────────┐  ┌────────────┐
│ lower_struct │  │ lower_expr.rs    │  │ lower_stmt   │  │ lower_call │
│   (NEW)      │  │   AccessIndex on │  │   Statement::│  │   sret + arg
│   member     │  │   struct slot;   │  │   Store on   │  │   pass for │
│   load/store │  │   Compose into   │  │   struct or  │  │   structs  │
│   memcpy     │  │   slot dest      │  │   member     │  │            │
└──────────────┘  └──────────────────┘  └──────────────┘  └────────────┘
                          │
                          ▼
                ┌──────────────────────┐
                │ lower_access.rs      │
                │   member-store       │
                │   through inout/out  │
                │   pointer            │
                └──────────────────────┘
```

## Data structures

### `AggregateLayout` (new — `naga_util.rs`)

Pure type-level layout, independent of any specific local:

```rust
pub(crate) struct AggregateLayout {
    pub kind: AggregateKind,
    pub total_size: u32,
    pub align: u32,
}

pub(crate) enum AggregateKind {
    Array {
        dimensions: SmallVec<[u32; 4]>,
        leaf_element_ty: Handle<Type>,
        leaf_stride: u32,
        element_count: u32,
    },
    Struct {
        members: Vec<MemberInfo>,
    },
}

pub(crate) struct MemberInfo {
    /// std430 byte offset from the struct base.
    pub byte_offset: u32,
    /// Naga member type handle (so we can recurse / look up nested layouts).
    pub naga_ty: Handle<Type>,
    /// `LpsType` of the member, cached for the slot-write primitive.
    pub lps_ty: lps_shared::LpsType,
    /// IR types in std430 order for the member (scalar/vector/matrix flattening).
    /// Empty for nested structs / arrays — those recurse via `naga_ty`.
    pub ir_tys: SmallVec<[IrType; 4]>,
}
```

Single constructor:

```rust
pub(crate) fn aggregate_layout(
    module: &Module,
    ty: Handle<Type>,
) -> Result<Option<AggregateLayout>, LowerError>
```

Returns `None` for non-aggregates (scalars/vectors/matrices). Returns
`Some(Array{..})` for `TypeInner::Array`, `Some(Struct{..})` for
`TypeInner::Struct`. Pointer types are unwrapped one level (caller
handles `Pointer { base }`).

### `AggregateInfo` (extended — `lower_ctx.rs`)

```rust
pub(crate) struct AggregateInfo {
    pub slot: AggregateSlot,        // unchanged
    pub layout: AggregateLayout,    // replaces dims/leaf_*/element_count/total_size
}
```

The five existing array-only fields move into `AggregateLayout::Array`.
Existing read sites (`info.dimensions`, `info.leaf_stride`, …) get
trivial accessor shims (`info.dimensions() -> &[u32]`, etc.) that
panic-or-error if called on a struct, plus a few rewrites where the call
site is unambiguously array-only (e.g. `lower_array_multidim`).

Rationale: this is the smallest possible diff that satisfies Q1
("extend, don't fork"). The existing array call sites stay readable.
Struct sites get `info.struct_members()` etc.

### Removed redundancies

- `lower_call::record_call_result_aggregate`'s manual array-shape
  extraction → replaced by `AggregateInfo::from_layout(layout, slot)`.
- `LowerCtx::aggregate_info_for_subscript_root`'s `flatten_array_type_shape`
  - `aggregate_size_and_align` → replaced by `aggregate_layout`.
- `naga_util::array_type_flat_components_for_value_coercions` and
  `array_ty_pointer_arg_ir_type` keep their existing roles; they're
  array-leaf utilities, not ABI deciders.

## Control-flow walkthroughs

### Lowering a struct local with an initializer

```glsl
Point p = Point(1.0, 2.0);
```

1. `LowerCtx::new` → `aggregate_layout(typeof(p))` returns
   `Struct { members: [{0, F32}, {4, F32}], total_size: 8 }`. Allocate
   a 8-byte slot, build `AggregateInfo { slot: Local(s), layout }`,
   insert into `aggregate_map`.
2. `LowerCtx::new`'s "init pass" sees `var.init = Some(compose_h)`,
   the local has an aggregate entry, so dispatches to
   `lower_aggregate_write::store_lps_value_into_slot(ctx, base=&slot,
offset=0, lps_ty=Struct{Point}, expr=compose_h)`.
3. The slot-write primitive sees `LpsType::Struct`, walks
   `Compose.components` in lockstep with `layout.members`, and for each
   member calls itself recursively with `(base, member.byte_offset,
member.lps_ty, component_h)`. Scalar leaves bottom out into
   `ensure_expr_vec` + typed `Store` per IR component.

### Lowering `p.x`

```glsl
return p.x;
```

`Expression::AccessIndex { base = Load { pointer = LocalVariable(p) },
index = 0 }`:

1. `lower_expr` recognises the pattern (struct local via aggregate_map).
2. Looks up `aggregate_map[p].layout.members[0]` → `byte_offset = 0`,
   `ir_tys = [F32]`.
3. Emits `Load { dst, base=&slot, offset=0 }`, returns the resulting
   single-vreg `VRegVec`.

(For nested access `triangle.a.x`, the same path applies recursively —
each `AccessIndex` peels one layer using the appropriate child layout.)

### Lowering `move_point(p, 5.0, 3.0)` — `inout` struct

1. `lower_call::lower_user_call` arg loop sees callee arg is
   `TypeInner::Pointer { base: Struct }`. Existing code already handles
   `TypeInner::Pointer { base }` via `call_arg_pointer_local` +
   `aggregate_map.get(lv)`. The only change is that
   `aggregate_map.get(p)` now returns a `Struct` info (the existing path
   only knew about arrays, but it just needs the slot pointer — that's
   the same).
2. Pass `aggregate_storage_base_vreg(ctx, &info.slot)` as the arg.
3. No copyback needed (callee writes directly through the pointer).

### Lowering `circle_area(circle)` — by-value struct `in` arg

```glsl
float circle_area(Circle c) { return 3.14159 * c.radius * c.radius; }
```

1. `LowerCtx::new` (callee side): `func.arguments[0].ty` is a struct.
   The new `TypeInner::Struct { .. }` arm of the param-walk pushes a
   `PendingInStructValueArg { arg_i, lv, layout }`, allocates one
   `IrType::Pointer` LPIR param. Mirrors the existing
   `PendingInArrayValueArg` arm.
2. After the param loop, we allocate the local slot and emit
   `Memcpy { dst = &local_slot, src = param_ptr, size = total_size }`
   (also mirrors arrays). Insert into `aggregate_map` as
   `AggregateInfo { slot: Local(s), layout }`.
3. `scan_param_argument_indices` already excludes "is array value"; we
   add an "is struct value" guard with the same effect → the param
   doesn't get added to `param_aliases`.
4. Caller side (`lower_user_call`): the existing arg loop's
   `else if matches!(callee_inner, TypeInner::Array { .. })` branch
   becomes "callee_inner is an aggregate (array OR struct)" via
   `aggregate_layout(...).is_some()`. Pulls
   `aggregate_storage_base_vreg(...)` from caller's `aggregate_map`.

### Lowering `Color test_param_struct_return() { return blend_colors(red, blue, 0.5); }`

Two interesting features stack here: caller has struct locals (`red`,
`blue`); both the inner and outer functions return structs (sret).

1. Outer function `test_param_struct_return` returns `Color` →
   `func_return_ir_types_with_sret` (via the consolidated layout query)
   sets up sret. `LowerCtx` allocates `red`, `blue` as struct slots.
2. Inner call `blend_colors(red, blue, 0.5)`:
   - `lower_user_call` sees callee returns aggregate → allocates an sret
     dest slot in the outer function, builds `AggregateInfo`, registers
     in `call_result_aggregates` (now via `from_layout`).
   - Args: callee args 0 and 1 are by-value struct → caller passes
     `&red_slot`, `&blue_slot`. Arg 2 is `f32` → flat scalar.
3. Outer function's `Statement::Return { value: Some(call_result) }` →
   `write_aggregate_return_into_sret` sees `Expression::CallResult`,
   pulls the slot from `call_result_aggregates`, emits
   `Memcpy { dst = sret.addr, src = &call_result_slot, size }`.

### Lowering `out_circle = process_circle(...)` — out-struct via `out` param

```glsl
void process_circle(in Circle input, out Circle output, inout Point center) {
    output = input;
    ...
}
```

Two new things in the callee body for M2:

- **`output = input;`** — whole-struct assignment via `out` pointer +
  by-value `in`. `lower_stmt::Statement::Store { pointer = output,
value = Load(input_slot) }` →
  `store_lps_value_into_slot(ctx, base=output_param_ptr, offset=0,
lps_ty=Struct{Circle}, expr=Load(input))`. The slot-write primitive's
  "memcpy fast path" recognises that the source is an existing
  slot-backed struct of matching layout and emits a single
  `Memcpy { dst=output_param_ptr, src=&input_slot, size }`.
- **`output.radius = output.radius * 2.0;`** — store to a member through
  `out` pointer. New code path in `lower_access.rs`:
  `Statement::Store { pointer = AccessIndex(output, member_idx), value }`.
  Resolve `pointer_args.get(arg_i_for_output)` → `Handle<Type>` of the
  pointee struct. Look up `aggregate_layout(pointee).Struct.members[
member_idx]` for offset and IR types. Emit per-IR-component `Store
{ base=output_param_ptr, offset=member.byte_offset + j*4, value }`.

### Lowering uniform struct member access (already works — preserved)

`load_lps_value_from_vmctx` recurses through `LpsType::Struct` for
member offsets. M2 doesn't change this path; we cross-check that it
still works with the new `aggregate_layout` in place (it shouldn't be
called for uniforms — uniforms are read via VMContext, not `aggregate_map`).

## Module / file boundaries

| File                                              | Change                                                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lps-frontend/src/naga_util.rs`                   | + `naga_type_to_ir_types` Struct arm; + `aggregate_layout` (replaces 5 inline checks); + AccessIndex on struct value in `expr_type_inner`/`expr_scalar_kind`; rewrite `func_return_ir_types_with_sret` to use `aggregate_layout`                                                                                                                                        |
| `lps-frontend/src/lower_ctx.rs`                   | `AggregateInfo { layout }`; struct-arm in `LowerCtx::new` param loop (mirrors `PendingInArrayValueArg`); struct-arm in local loop (allocate slot from layout); `scan_param_argument_indices` guards struct value params; `aggregate_info_for_subscript_root` uses `aggregate_layout`                                                                                    |
| `lps-frontend/src/lower_aggregate_write.rs` (NEW) | `store_lps_value_into_slot(ctx, base, offset, lps_ty, expr_h)` — single source of truth for "write LpsType into (base, offset)" with slot-source memcpy fast path. Used by array init (refactor) and struct compose / store.                                                                                                                                            |
| `lps-frontend/src/lower_struct.rs` (NEW)          | Thin layer: `load_struct_member_to_vregs(ctx, info, member_idx) -> VRegVec`, `materialise_struct_rvalue_to_temp_slot(ctx, expr_h) -> AggregateInfo` (for call args / nested compose with no destination slot). Whole-struct memcpy uses `LpirOp::Memcpy` directly.                                                                                                      |
| `lps-frontend/src/lower_array.rs`                 | `lower_array_initializer` rewritten on top of `store_lps_value_into_slot`; `zero_fill_array_slot` keeps existing shape (no expr); `aggregate_storage_base_vreg` unchanged.                                                                                                                                                                                              |
| `lps-frontend/src/lower_expr.rs`                  | `Expression::AccessIndex` arm for `LocalVariable(struct)` and `FunctionArgument(struct via Pointer)`; `Expression::Load` of a struct local → `UnsupportedExpression` (Q4) so callers must dispatch on `aggregate_map`; `Expression::Compose { ty: Struct }` writes into a destination slot (when caller supplies one) else allocates a temp slot and returns a pointer. |
| `lps-frontend/src/lower_stmt.rs`                  | `Statement::Store` of a whole struct local / via `out` pointer of a struct → `store_lps_value_into_slot` (memcpy fast path picks up slot-source); `Statement::Store` of a struct member on a local → typed `Store` at member offset; `Statement::Return` with struct value → existing `write_aggregate_return_into_sret`, now layout-driven.                            |
| `lps-frontend/src/lower_access.rs`                | New arm for member-store through `inout`/`out` struct pointer (uses `pointer_args` + `aggregate_layout`).                                                                                                                                                                                                                                                               |
| `lps-frontend/src/lower_call.rs`                  | `record_call_result_aggregate` and `write_aggregate_return_into_sret` rewritten to use `aggregate_layout`; arg-loop "is aggregate" check replaces `matches!(callee_inner, TypeInner::Array { .. })` with `aggregate_layout(callee_arg.ty).is_some()`; struct rvalue arg handled by routing through `materialise_struct_rvalue_to_temp_slot`.                            |
| `lps-frontend/src/lib.rs`                         | `mod lower_struct;`, `mod lower_aggregate_write;`.                                                                                                                                                                                                                                                                                                                      |

## Filetest enablement (M2 acceptance)

After the lowering work lands:

1. From the workspace root, run `scripts/filetests.sh --fix` (or
   `LP_FIX_XFAIL=1`) against the struct corpus — same runner as
   `just test-filetests`:
   - `lps-filetests/filetests/struct/*.glsl`
   - `lps-filetests/filetests/function/{param,return}-struct.glsl`
   - `lps-filetests/filetests/uniform/struct.glsl`
   - `lps-filetests/filetests/global/type-struct.glsl`
2. Verify `wasm.q32`, `rv32c.q32`, `rv32n.q32` all pass on every test
   in the corpus. Diverge between rv32c and rv32n → backend bug, fix
   in M2.
3. `jit.q32` markers untouched — not an acceptance target (see roadmap).
4. Per Q10-i β: a non-struct-related bug surfaced by un-ignoring may be
   re-marked with `// TODO(bug-N): <reason>` and a filed issue, but the
   bias is to fix.

## Risks and mitigations

- **R-A. `AggregateInfo`-struct migration touches every array call site.**
  Mitigation: shim accessors (`info.dimensions()`, `info.leaf_stride()`)
  preserve current code shape; the only forced rewrites are the five
  ABI-decision sites that move to `aggregate_layout`.
- **R-B. `Compose { ty: Vector }` inside `Compose { ty: Struct }`.**
  E.g. `Color(vec3(r,g,b), 1.0)`. `store_lps_value_into_slot` dispatches
  on the _member's_ `LpsType`, not on the component's expression shape;
  vector members fall into the scalar-leaf path and emit per-component
  Stores at `member.byte_offset + j*4`. Confirmed safe.
- **R-C. Struct rvalue at non-call sites we forgot.** Anywhere a struct
  value flows without a destination slot today errors out with
  `UnsupportedExpression`. After M2, the bias is to allocate a temp
  slot via `materialise_struct_rvalue_to_temp_slot`. The places we know
  need it: direct call args, return-value `write_aggregate_return_into_sret`
  (already temp-slots arrays). Anything else surfaces as a filetest
  failure during the enable phase.
- **R-D. Naga emits `Compose` differently for structs vs vectors.**
  Verified by reading `function/param-struct.glsl` (Color uses
  `Color(vec3(r,g,b), a)`) and `struct/constructor-nested.glsl`. Walker
  is dispatch-on-member-`LpsType`, so this is fine — but the very first
  implementation phase prints IR for `constructor-nested.glsl` to
  cross-check before scaling up.
- **R-E. RV32 backend gaps surface late.** Mitigation: each phase
  ends with an enable-and-run sub-step on the in-scope subset, not all
  in one big bang at the end (see phase plan in 02-…).

## Phase breakdown (preview)

Five implementation steps — see `02-` … `06-` in this directory (file
numbers align with the old milestone outline; there is no `01-` phase
file beyond this design doc):

1. **02 — `aggregate_layout` + `AggregateInfo` migration** (no behaviour
   change). Pure refactor: introduce `AggregateLayout`, migrate the five
   ABI-decision sites + `LowerCtx::new` array path. Acceptance: existing
   array filetests stay green; no struct work yet.
2. **03 — `lower_aggregate_write` + `lower_array_initializer` refactor.**
   Unify the slot-write primitive on top of `aggregate_layout`.
   Acceptance: array filetests still green; new internal API used by
   `lower_array_initializer` only.
3. **04 — Struct lowering (frontend core).** `naga_util` Struct arms;
   `LowerCtx::new` struct local + by-value `in` param; `lower_struct.rs`;
   `lower_expr` AccessIndex/Compose/Load on structs; `lower_stmt`
   whole-struct + member store; `lower_access` member-store through
   pointer. Print-IR sanity check on `constructor-nested.glsl`
   (`scripts/filetests.sh --debug …`). Acceptance: struct corpus
   subset that does not require struct call ABI passes on default
   targets.
4. **05 — Struct call ABI.** `lower_call` arg/result for structs;
   `func_return_ir_types_with_sret` struct path; struct rvalue temp-slot
   materialisation. Acceptance: `function/param-struct.glsl`,
   `function/return-struct.glsl` on default targets.
5. **06 — Enable + sweep.** `scripts/filetests.sh --fix` on the
   struct corpus; resolve fallout; update roadmap + `summary.md`.
