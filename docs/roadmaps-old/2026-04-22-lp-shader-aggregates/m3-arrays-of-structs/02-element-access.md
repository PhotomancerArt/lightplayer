# Phase 2 — Element Access: Peeler, load/store helpers, AccessIndex dispatch

## Goal

Enable member access into array-of-struct elements: `ps[i].x`, `ps[i].pos.x`,
`ps[i] = q`. After this phase, the core array-of-struct functionality works for
slot-backed locals.

## Files to modify

| File | Changes |
|------|---------|
| `lp-shader/lps-frontend/src/lower_struct.rs` | Add `peel_arrayofstruct_chain`, `load_array_struct_element`, `store_array_struct_element`. |
| `lp-shader/lps-frontend/src/lower_expr.rs` | In `AccessIndex` lowering, add early dispatch: try `peel_arrayofstruct_chain`, on hit call `load_array_struct_element` and cache result. |
| `lp-shader/lps-frontend/src/lower_stmt.rs` | In `Store` lowering, add early dispatch for LHS: try `peel_arrayofstruct_chain`, on hit call `store_array_struct_element`. |
| `lp-shader/lps-frontend/src/lower_array.rs` | Whole-element assignment `ps[i] = q`: detect in `lower_stmt` Store path (via the peeler with empty member_chain) and emit `Memcpy` or call `store_lps_value_into_slot`. |

## New types and helpers (lower_struct.rs)

```rust
/// Index into an array element: constant or dynamic (already computed VReg).
pub(crate) enum ElementIndex {
    Const(u32),
    Dynamic(naga::Handle<naga::Expression>),  // expression handle, not VReg
}

/// Result of peeling an AccessIndex/Access chain that resolves to an array-of-struct element.
pub(crate) struct ArrayOfStructChain {
    /// The array aggregate (contains leaf_layout as a Struct).
    pub info: crate::lower_ctx::AggregateInfo,
    /// Index into the array (const or dynamic expression handle).
    pub index: ElementIndex,
    /// Struct member indices applied after the array index (may be empty for whole-element).
    pub member_chain: smallvec::SmallVec<[u32; 4]>,
    /// Layout of the struct leaf type (cached for member offset lookups).
    pub leaf_layout: crate::naga_util::AggregateLayout,
}

/// Walk expr looking for: (struct AccessIndex)* → array Access/AccessIndex → (struct AccessIndex)*
/// Returns None if the chain doesn't match array-of-struct.
pub(crate) fn peel_arrayofstruct_chain(
    ctx: &crate::lower_ctx::LowerCtx<'_>,
    expr: naga::Handle<naga::Expression>,
) -> Option<ArrayOfStructChain>;

/// Load from array-of-struct element + optional member chain.
/// Returns VRegs for the final leaf type (scalar/vector after member_chain, or whole struct if empty).
pub(crate) fn load_array_struct_element(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    chain: &ArrayOfStructChain,
) -> Result<crate::lower_ctx::VRegVec, crate::lower_error::LowerError>;

/// Store into array-of-struct element + optional member chain.
pub(crate) fn store_array_struct_element(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    chain: &ArrayOfStructChain,
    rhs: naga::Handle<naga::Expression>,
) -> Result<(), crate::lower_error::LowerError>;
```

## Peeler logic

The Naga expression graph for array-of-struct access can have struct member
steps on either side of the array index:

- `ps[i].x` → `AccessIndex(base: Access(base: ps_local, index: i_expr), index: 0)`
- `ps[i].pos.x` → longer chain of AccessIndex after the Access
- `s.ps[i].x` → AccessIndex(s_local, ps_field_idx) → Access(..., i_expr) → AccessIndex(..., x_idx)

The peeler walks **upward** (from the expression toward its bases) looking for:

1. Zero or more `AccessIndex` steps (struct members) — record their indices in
   `prefix_chain`.
2. Exactly one array index step: either `Access { base, index }` (dynamic)
   or `AccessIndex { base, index }` on an array-typed base (constant index).
3. Zero or more `AccessIndex` steps after — record in `suffix_chain`.
4. The root: either a `LocalVariable` that maps to an `AggregateInfo` with a
   struct leaf, or a `Load { pointer: LocalVariable }` of such a local, or
   (Phase 2 only local; Phase 3 adds outer-struct-field) a struct field that
   is itself an array-of-struct.

For Phase 2, we support only the simple local-root case:
- Root is `LocalVariable(lv)` where `ctx.aggregate_map.get(&lv)` exists and
  the `AggregateInfo` has `leaf_element_ty` that resolves (via
  `aggregate_layout`) to a `Struct` kind.

The `member_chain` returned is `prefix_chain + suffix_chain` (flattened into
one sequence of member indices). The `ArrayOfStructChain` captures the
`AggregateInfo` of the array (not the struct), the index into the array, and
the flattened member chain.

## Load helper

```rust
pub(crate) fn load_array_struct_element(
    ctx: &mut LowerCtx<'_>,
    chain: &ArrayOfStructChain,
) -> Result<VRegVec, LowerError> {
    // 1. Compute element address via Phase 1's array_element_address
    let elem_addr = crate::lower_array::array_element_address(
        ctx, &chain.info,
        match chain.index {
            ElementIndex::Const(i) => crate::lower_array::ElementIndex::Const(i),
            ElementIndex::Dynamic(expr_h) => {
                let vreg = ctx.ensure_expr(expr_h)?;
                crate::lower_array::ElementIndex::Dynamic(vreg)
            }
        }
    )?;

    // 2. Walk member_chain to accumulate constant byte offset into the struct leaf
    let mut member_offset = 0u32;
    let mut current_layout = &chain.leaf_layout;
    for &member_idx in &chain.member_chain {
        let members = current_layout.struct_members()
            .ok_or_else(|| LowerError::Internal("load: not a struct".into()))?;
        let m = members.get(member_idx as usize)
            .ok_or_else(|| LowerError::Internal("load: bad member index".into()))?;
        member_offset += m.byte_offset;
        // If this member is itself a struct and there are more indices, descend
        if member_idx + 1 < chain.member_chain.len() as u32 {
            if matches!(m.lps_ty, LpsType::Struct { .. }) {
                current_layout = crate::naga_util::aggregate_layout(ctx.module, m.naga_ty)?
                    .ok_or_else(|| LowerError::Internal("load: nested struct layout".into()))?;
            } else {
                return Err(LowerError::Internal("load: too many member indices".into()));
            }
        }
    }

    // 3. Determine the final leaf type after member_chain
    let final_naga_ty = if chain.member_chain.is_empty() {
        chain.info.leaf_element_ty()
    } else {
        // Look up the type of the last member
        // ... (use accumulated current_layout info)
    };

    // 4. Emit Load(s) at elem_addr + member_offset
    let final_inner = &ctx.module.types[final_naga_ty].inner;
    let ir_tys = naga_type_to_ir_types(ctx.module, final_inner)?;
    let mut out = VRegVec::new();
    for (j, ty) in ir_tys.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: elem_addr,
            offset: member_offset + (j as u32) * 4,
        });
        out.push(dst);
    }
    Ok(out)
}
```

Actually, the member chain walking logic is more complex than needed for M3.
Simpler approach: the peeler only recognizes chains where all struct member
steps are **after** the array index (the common case: `ps[i].x`). For
`ps[i].pos.x` (nested struct), the peeler returns the chain up to the first
struct level, and the existing `AccessIndex` lowering recurses naturally.

Revised simpler contract:

```rust
pub(crate) struct ArrayOfStructChain {
    pub info: AggregateInfo,           // the array
    pub index: ElementIndex,           // into array
    pub first_member: Option<u32>,   // None = whole element, Some(0) = .x, etc.
}
```

The peeler recognizes:
- `Access { base: local_var, index }` → array root is local, index is dynamic
- `AccessIndex { base: Access { base: local_var, index }, index: m }` → array
  root is local, index is dynamic, first_member = m

For const index: `AccessIndex { base: AccessIndex { base: local_var, index: const_idx }, index: m }`
where the inner AccessIndex has an array-typed base.

The full `ps[i].pos.x` decomposes into:
- peeler returns `first_member = pos_idx`
- outer `AccessIndex` lowering sees the result, notices it's a struct type,
  and recurses into member `x` via existing M2 struct handling

This keeps the peeler simple and shares the recursive struct member handling
with M2.

## Store helper

Similar to load, but:
- For `first_member = Some(m)`: emit Store(s) at the computed address.
- For `first_member = None` (whole element): try `try_memcpy_aggregate_expr`
  fast path (if RHS is slot-backed struct of same type), else call
  `store_lps_value_into_slot(..., Some(&leaf_layout))`.

## Dispatch in lower_expr (AccessIndex)

In `lower_expr.rs`, the `AccessIndex` match arm (around line 191) should:

```rust
Expression::AccessIndex { base, index } => {
    // Try array-of-struct chain first
    if let Some(chain) = crate::lower_struct::peel_arrayofstruct_chain(ctx, expr) {
        return crate::lower_struct::load_array_struct_element(ctx, &chain);
    }
    // ... existing AccessIndex arms (scalar, vector, matrix, struct pointer, etc.)
}
```

Note: `peel_arrayofstruct_chain` takes the full expression `expr` (the
AccessIndex handle), not just `base`, because it needs to see the full chain.

## Dispatch in lower_stmt (Store)

In `lower_stmt.rs`, where `Statement::Store` is handled:

```rust
Statement::Store { pointer, value } => {
    // Try array-of-struct chain on the pointer
    if let Some(chain) = crate::lower_struct::peel_arrayofstruct_chain(ctx, pointer) {
        return crate::lower_struct::store_array_struct_element(ctx, &chain, value);
    }
    // ... existing Store handling
}
```

Wait: `pointer` in a Store is the LHS expression being stored **into**. For
`ps[i].x = 5.0`, the `pointer` is the `AccessIndex` expression representing
`ps[i].x`. So yes, passing `pointer` to the peeler is correct.

## Whole-element assignment

For `ps[i] = q;` where `q` is a struct local:
- Peeler returns `first_member = None` (whole element)
- `store_array_struct_element` sees this and:
  1. Computes element address
  2. Tries `try_memcpy_aggregate_expr` with the RHS — if RHS is a slot-backed
     struct local of the same type, emits Memcpy
  3. Else calls `store_lps_value_into_slot` with the element address as base
     and the RHS expression

## Acceptance criteria

- `ps[i].x` and `ps[i].y` work (dynamic index member read)
- `ps[0].x` works (const index member read)
- `ps[i].x = v` works (dynamic index member write)
- `ps[i] = Point(1.0, 2.0)` works (whole-element assignment via Compose)
- `ps[i] = q` works (whole-element assignment from struct local via Memcpy)

Filetests to add (all `// test run`):
- `array/of-struct/local-member-rw.glsl` — read/write members with const and dynamic indices
- `array/of-struct/local-whole-assign.glsl` — whole-element assignment

## Out of scope

- `ps[i].pos.x` (nested struct member) — if not handled naturally by recursion,
  document as follow-up
- `s.ps[i].x` (outer struct field is array-of-struct) — Phase 3
- Array-of-struct as function return — already handled by sret path if Phase 1
  permits the type

## Testing

```glsl
// array/of-struct/local-member-rw.glsl
// test run
// expected: 1 2 3 4

struct Point { float x; float y; };

void main() {
    Point ps[2];
    ps[0].x = 1.0;
    ps[0].y = 2.0;
    ps[1].x = 3.0;
    ps[1].y = 4.0;
    output_f32(ps[0].x);
    output_f32(ps[0].y);
    output_f32(ps[1].x);
    output_f32(ps[1].y);
}
```

```glsl
// array/of-struct/local-whole-assign.glsl
// test run
// expected: 5 6

struct Point { float x; float y; };

void main() {
    Point ps[2];
    Point q = Point(5.0, 6.0);
    ps[0] = q;
    output_f32(ps[0].x);
    output_f32(ps[0].y);
}
```

```glsl
// array/of-struct/local-compose-assign.glsl
// test run
// expected: 7 8

struct Point { float x; float y; };

void main() {
    Point ps[2];
    ps[0] = Point(7.0, 8.0);
    output_f32(ps[0].x);
    output_f32(ps[0].y);
}
```
