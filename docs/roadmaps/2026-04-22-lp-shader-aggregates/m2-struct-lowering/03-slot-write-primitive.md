# Phase 03 — Unified slot-write primitive (`store_lps_value_into_slot`)

**Tags:** sub-agent: yes, parallel: no (depends on phase 02)

## Scope of phase

Introduce a single primitive that writes a value of a given `LpsType`
into a `(base, byte_offset)` location inside a slot, then **rewrite
`lower_array_initializer` on top of it**. No struct work yet — the
primitive only exercises its scalar/vector/matrix/array arms in this
phase. Padding bytes are left undefined (std430 never observes them).

This phase exists so phase 04 (struct frontend) can drop its `Compose
{ ty: Struct }` lowering directly onto an existing, array-tested
primitive.

### Out of scope

- Anything `TypeInner::Struct`. The primitive's struct arm may be
  stubbed with `LowerError::UnsupportedType("phase 04")`.
- Any change to `LowerCtx::new`'s array-init path beyond it now calling
  the new primitive.
- Any change to filetests.

## Code organization reminders

- One concept per file.
- Helpers at the bottom of each file; abstract entry points at the top.
- Group related functionality.
- Any temporary code must have a `TODO` comment.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. No struct lowering in this phase.
- Do **not** suppress warnings — fix them.
- Do **not** weaken or skip existing tests.
- Stop and report on ambiguity rather than improvising.

## Implementation details

### 1. New module `lower_aggregate_write.rs`

Create `lp-shader/lps-frontend/src/lower_aggregate_write.rs`. Wire it
in `lib.rs`:

```rust
mod lower_aggregate_write;
```

Public surface:

```rust
/// Write the value of `expr_h` (typed as `lps_ty`) into the byte range
/// `[base + offset, base + offset + sizeof(lps_ty))`. Recurses for nested
/// aggregate `lps_ty`. Padding bytes implied by std430 are left undefined.
///
/// Memcpy fast path: if `expr_h` reduces to a slot-backed aggregate of
/// matching `lps_ty`, this emits a single `LpirOp::Memcpy` instead of a
/// component-wise unpack.
pub(crate) fn store_lps_value_into_slot(
    ctx: &mut LowerCtx<'_>,
    base: VReg,
    offset: u32,
    lps_ty: &lps_shared::LpsType,
    expr_h: Handle<Expression>,
) -> Result<(), LowerError>;
```

Internal dispatch:

- `LpsType::Float | Int | Uint | Bool` → one IR component, one `Store`
  at `(base, offset)`.
- `LpsType::Vec2/3/4 | IVec*/UVec*/BVec*` → call `ensure_expr_vec` +
  `coerce_assignment_vregs`; for each `IrType` component emit
  `Store { base, offset: offset + j*4, value }`.
- `LpsType::Mat2/3/4` → same pattern; ordering follows
  `naga_type_to_ir_types` for matrices (column-major std430).
- `LpsType::Array { element, len }` → for each `i in 0..len` recurse
  with `(base, offset + i * stride_of(element), element, ith_component)`.
  Stride is `lps_shared::layout::type_stride(element)`. Source
  components come from `collect_flat_compose_components` for `Compose`
  expressions, identical to today's `lower_array_initializer` flatten
  logic. (Move that helper into this module, or keep it in
  `lower_array.rs` and re-export — your call.)
- `LpsType::Struct { .. }` → `Err(LowerError::UnsupportedType(
  String::from("store_lps_value_into_slot: struct path lands in M2 phase 04"))).`

#### Memcpy fast path

Before any per-component dispatch, peel `expr_h` for slot-backed
aggregate sources. Currently slot-backed aggregates are arrays
(`aggregate_map`-tracked locals + `call_result_aggregates`). Match
shapes:

- `Expression::LocalVariable(lv)` with `aggregate_map.get(lv)` of array
  kind matching `lps_ty` → `Memcpy` from
  `aggregate_storage_base_vreg(ctx, &info.slot)` of `info.layout.total_size`
  into `(base, offset)`.
- `Expression::Load { pointer = LocalVariable(lv) }` — same.
- `Expression::CallResult(_)` with a matching `call_result_aggregates`
  entry — same.

For shape-match, compare `info.layout.total_size` and the leaf type of
the array kind against the `LpsType::Array { element, len }`. If
shapes don't match (e.g. assigning a `vec3[2]` to a slot typed as
`vec4[2]`), error — that's not a valid GLSL assignment.

### 2. Rewrite `lower_array_initializer`

Replace the body of
`lp-shader/lps-frontend/src/lower_array.rs::lower_array_initializer`:

Current shape:

```rust
match &ctx.func.expressions[init_h] {
    Expression::ZeroValue(_) => zero_fill_array(ctx, ctx.module, info),
    Expression::Compose { .. } => { /* flatten + per-element store */ },
    _ => Err(...),
}
```

New shape (for phase 03 only the `Array` arm exercises the primitive):

```rust
pub(crate) fn lower_array_initializer(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    init_h: Handle<Expression>,
) -> Result<(), LowerError> {
    if matches!(&ctx.func.expressions[init_h], Expression::ZeroValue(_)) {
        return zero_fill_array(ctx, ctx.module, info);
    }
    let base = aggregate_storage_base_vreg(ctx, &info.slot)?;
    // Look up the LpsType once for this aggregate (use the type handle from
    // `LowerCtx::new`'s var.ty / arg.ty path; thread through `info` if needed).
    let lps_ty = lps_type_of_aggregate_info(ctx.module, info)?;
    crate::lower_aggregate_write::store_lps_value_into_slot(
        ctx, base, /*offset=*/ 0, &lps_ty, init_h,
    )
}
```

If `info` doesn't already carry a Naga type handle suitable for
`naga_type_handle_to_lps`, add a `pub naga_ty: Handle<Type>` field on
`AggregateInfo` and populate it where each `AggregateInfo` is built
(one new line per construction site — there are three: `LowerCtx::new`
local arm, `LowerCtx::new` pending-in-array-value-param resolution,
and `lower_call::record_call_result_aggregate`). This is the
single field addition this phase makes to `AggregateInfo`.

### 3. Keep `zero_fill_array` / `zero_fill_array_slot` unchanged

These have no expression input; they iterate `info.layout.kind.Array`
fields directly. Don't refactor them onto the new primitive in this
phase.

### 4. Don't break existing array tests

Special cases to preserve:

- Multi-dim flatten: `Compose` with depth > 1 is flattened by
  `collect_flat_compose_components(func, init_h, depth)` where
  `depth = dimensions.len() - 1`. The new primitive's array arm handles
  this by recursing into its own `LpsType::Array { element, .. }` arm
  with the inner-array `LpsType` as `element`. Verify by reading the
  IR for `array/declare-multidim.glsl` after the rewrite (expect: same
  store pattern as before).
- Initializer shorter than `element_count`: tail elements zeroed (see
  `zero_element_at_byte_offset` in `lower_array.rs`). Preserve in the
  new primitive's array arm.
- `coerce_assignment_vregs` is called per leaf — keep that.

## Validate

From the workspace root:

```sh
just check
```

Then targeted filetests (from workspace root):

```sh
./scripts/glsl-filetests.sh array/
./scripts/glsl-filetests.sh function/return-array.glsl
./scripts/glsl-filetests.sh function/param-array.glsl
```

All array-related filetests must remain in their pre-phase pass/fail
state on every default target. Any delta is a refactor bug — investigate.

If practical, print IR for one multi-dim array test
(`array/declare-multidim.glsl`) before and after the rewrite and confirm
the store sequence is identical:

```sh
./scripts/glsl-filetests.sh --debug array/declare-multidim.glsl
```

(Diffing the printed `=== LPIR ===` section is the fastest way to catch
a fast-path miss.)
