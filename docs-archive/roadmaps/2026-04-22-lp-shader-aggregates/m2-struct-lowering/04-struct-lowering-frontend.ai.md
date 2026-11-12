# Phase 04 — Struct lowering (frontend core, no calls yet)

**Tags:** sub-agent: yes, parallel: no (depends on phase 03)

## Scope of phase

Land first-class struct support for **within-function** lowering:
struct locals, member access (`AccessIndex`), `Compose` for structs,
whole-struct assignment between locals, member stores. **No call-ABI
changes** — by-value struct args and struct returns are still
unsupported and will error out (phase 05 enables them).

Acceptance proxy for this phase: every test in
`lps-filetests/filetests/struct/` that does not cross a function
boundary with a struct value passes on `wasm.q32`, `rv32c.q32`,
`rv32n.q32`. Tests that _do_ require struct calls (some of
`constructor-nested.glsl` etc. internally just initialize-then-read,
which works without calls) continue to pass; tests that need struct
returns still fail. That's expected for phase 04.

### Out of scope

- `lower_call.rs` changes for structs (phase 05).
- `func_return_ir_types_with_sret` struct path (phase 05).
- `function/param-struct.glsl` and `function/return-struct.glsl`
  acceptance (phase 05).
- `--fix` of any filetest annotations (phase 06).

## Code organization reminders

- One concept per file.
- Helpers at the bottom of each file; abstract entry points at the top.
- Group related functionality.
- Any temporary code must have a `TODO` comment.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope into call-ABI work.
- Do **not** suppress warnings — fix them.
- Do **not** weaken or skip existing tests.
- Stop and report on ambiguity rather than improvising.

## Implementation details

### 1. `naga_util` struct arms

In `lp-shader/lps-frontend/src/naga_util.rs`:

#### 1a. `naga_type_to_ir_types` Struct arm

Add a `TypeInner::Struct { members, .. }` arm that returns the std430-
ordered concatenation of `naga_type_to_ir_types(members[i].ty)` for each
member. **Used only by value-coercion paths**; the slot-write primitive
uses `MemberInfo.ir_tys` (see step 2 below) so it can place each
member at its own std430 offset, which the flat list cannot represent.

#### 1b. `expr_type_inner` / `expr_scalar_kind` AccessIndex arms

Add `TypeInner::Pointer { base: Struct { members, .. } }` and the value
struct path to the `AccessIndex { base, index }` match: result type is
`module.types[members[index].ty].inner.clone()`. Mirror in
`expr_scalar_kind` where structurally analogous.

#### 1c. `aggregate_layout` Struct arm

Replace the phase-02 stub with the real implementation:

```rust
TypeInner::Struct { members, .. } => {
    let mut out_members = Vec::with_capacity(members.len());
    let mut current_offset = 0u32;
    let mut max_align = 1u32;
    let lps_ty = crate::naga_types::naga_type_handle_to_lps(module, ty)?;
    let lps_shared::LpsType::Struct { members: lps_members, .. } = &lps_ty else {
        unreachable!();
    };
    for (i, m) in members.iter().enumerate() {
        let lps_member_ty = lps_members[i].ty.clone();
        let member_align = lps_shared::layout::type_alignment(&lps_member_ty, R);
        let byte_offset = align_up(current_offset, member_align);
        let ir_tys = match &module.types[m.ty].inner {
            TypeInner::Scalar(_) | TypeInner::Vector { .. } | TypeInner::Matrix { .. } => {
                naga_type_to_ir_types(&module.types[m.ty].inner)?.into()
            }
            _ => SmallVec::new(), // nested aggregate; recurse via naga_ty
        };
        out_members.push(MemberInfo {
            byte_offset,
            naga_ty: m.ty,
            lps_ty: lps_member_ty.clone(),
            ir_tys,
        });
        current_offset = byte_offset + lps_shared::layout::type_size(&lps_member_ty, R);
        max_align = max_align.max(member_align);
    }
    let total_size = align_up(current_offset, max_align);
    Ok(Some(AggregateLayout {
        kind: AggregateKind::Struct { members: out_members },
        total_size,
        align: max_align,
    }))
}
```

Cross-check the produced offsets against the existing
`lower_aggregate_layout::aggregate_size_and_align` for the test struct
in `std430_struct_vec3_float`. They must match.

`MemberInfo` lives on `AggregateLayout::Struct`; struct from phase 02
plan/design doc.

### 2. `LowerCtx::new` — struct local + by-value `in` struct param

In `lp-shader/lps-frontend/src/lower_ctx.rs`:

#### 2a. Param loop — add `TypeInner::Struct` arm

Mirror the existing `TypeInner::Array { .. }` arm exactly. Build a new
`PendingInStructValueArg { arg_i, lv, layout }` (or extend
`PendingInArrayValueArg` to a unified `PendingInAggregateValueArg`
carrying `AggregateLayout` — your call, but the unified shape is
preferable). Allocate one `IrType::Pointer` LPIR param. After the param
loop, allocate the local slot, emit `Memcpy { dst = &local_slot, src
= param_ptr, size = layout.total_size }`, register
`AggregateInfo { slot: AggregateSlot::Local(s), layout }` in
`aggregate_map`.

`local_for_in_array_value_param` finds the `LocalVariable` matching the
param name. Generalise (or duplicate to `local_for_in_struct_value_param`)
for struct types — the matching logic is identical (name + type).

#### 2b. Local-variable loop — add `TypeInner::Struct` arm

Mirror the existing `TypeInner::Array { .. }` arm:

```rust
TypeInner::Struct { .. } => {
    let layout = aggregate_layout(module, var.ty)?
        .expect("struct layout");
    let slot = fb.alloc_slot(layout.total_size);
    aggregate_map.insert(
        lv_handle,
        AggregateInfo { slot: AggregateSlot::Local(slot), layout, naga_ty: var.ty },
    );
}
```

(Phase 03 added `naga_ty` to `AggregateInfo`. If not, add it here.)

The init pass at the bottom of `LowerCtx::new` already dispatches via
`aggregate_map.get(lv).cloned()` → `lower_array::lower_array_initializer`.
Generalise the dispatch:

```rust
if let Some(info) = ctx.aggregate_map.get(&lv_handle).cloned() {
    let lps_ty = naga_types::naga_type_handle_to_lps(ctx.module, info.naga_ty)?;
    let base = aggregate_storage_base_vreg(&mut ctx, &info.slot)?;
    crate::lower_aggregate_write::store_lps_value_into_slot(
        &mut ctx, base, 0, &lps_ty, init_h,
    )?;
    continue;
}
```

The slot-write primitive's struct arm (newly enabled in step 4 below)
handles `Compose { ty: Struct }`. The array arm is unchanged from
phase 03.

The "uninit struct → zero-fill" mirror: extend the post-init zero-fill
loop (currently only zeros arrays) to also zero structs without
initializers. Implementation: walk `layout.members` and emit
per-IR-component `Store` of zero at each `member.byte_offset + j*4`.
(For nested members, recurse via `aggregate_layout(member.naga_ty)?`.)

#### 2c. `scan_param_argument_indices` — exclude struct value params

Add a `is_struct_val` clause alongside `is_array_val`:

```rust
let is_struct_val = arg_ty.is_some_and(|h| {
    matches!(&module.types[h].inner, TypeInner::Struct { .. })
});
if !is_ptr && !is_array_val && !is_struct_val {
    m.insert(*lv, *idx);
}
```

This keeps struct-value params out of `param_aliases` so the local
finds its `aggregate_map` entry instead.

### 3. New module `lower_struct.rs`

Create `lp-shader/lps-frontend/src/lower_struct.rs`. Wire it in
`lib.rs`. Public surface (thin — most struct work lives in the
shared slot-write primitive):

```rust
/// Load IR-component vregs for a single struct member from a slot-backed
/// struct. Used by `lower_expr::AccessIndex` for the leaf-member case.
pub(crate) fn load_struct_member_to_vregs(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    member_idx: usize,
) -> Result<VRegVec, LowerError>;

/// Allocate a temp slot, materialise an aggregate rvalue (Compose or
/// CallResult expression) into it, return an `AggregateInfo` describing
/// the temp. Used by `lower_call` (phase 05) and `lower_expr::Compose`
/// when no destination slot is supplied.
pub(crate) fn materialise_aggregate_rvalue_to_temp_slot(
    ctx: &mut LowerCtx<'_>,
    expr_h: Handle<Expression>,
    layout: AggregateLayout,
    naga_ty: Handle<Type>,
) -> Result<AggregateInfo, LowerError>;
```

`materialise_aggregate_rvalue_to_temp_slot`: `fb.alloc_slot(layout.total_size)`

- `SlotAddr` + dispatch into `store_lps_value_into_slot` with the slot
  base / `lps_ty`. Per Q5, no slot reuse — every call allocates fresh.

### 4. `store_lps_value_into_slot` — enable Struct arm

In `lp-shader/lps-frontend/src/lower_aggregate_write.rs`, replace the
phase-03 stub for `LpsType::Struct`:

```rust
LpsType::Struct { members: lps_members, .. } => {
    // Resolve the matching AggregateLayout::Struct so we have byte_offset
    // / ir_tys per member. The Naga type handle isn't directly available
    // here — accept it as an additional argument (or re-look-up via the
    // expression's Naga type when expr_h is a Compose). Cleanest: extend
    // the primitive signature to take `Option<&AggregateLayout>` and pass
    // it explicitly from each call site. (Locals/init pass have it via
    // `AggregateInfo.layout`. Compose call sites resolve it from
    // `Expression::Compose { ty }` via `aggregate_layout`.)
    match &ctx.func.expressions[expr_h] {
        Expression::Compose { components, .. } => {
            for (i, &comp) in components.iter().enumerate() {
                let m = &layout_struct.members[i];
                store_lps_value_into_slot(
                    ctx, base, offset + m.byte_offset, &m.lps_ty, comp,
                )?;
            }
            Ok(())
        }
        Expression::ZeroValue(_) => zero_struct_into_slot(ctx, base, offset, layout_struct),
        Expression::LocalVariable(_) | Expression::Load { .. } | Expression::CallResult(_) => {
            // Memcpy fast path — handled by the primitive's pre-dispatch
            // peeling step. Reaching here means the source isn't slot-backed
            // (which shouldn't happen for a struct rvalue).
            Err(LowerError::Internal(String::from(
                "store_lps_value_into_slot: struct source must be slot-backed",
            )))
        }
        _ => Err(LowerError::UnsupportedExpression(format!(
            "struct compose source: {:?}", ctx.func.expressions[expr_h]
        ))),
    }
}
```

The Memcpy fast path from phase 03 already handles `LocalVariable` /
`Load(LocalVariable)` / `CallResult` source pattern → emits one
`LpirOp::Memcpy` instead of recursing. Confirm it triggers correctly
for struct sources after this phase enables struct entries in
`aggregate_map`.

### 5. `lower_expr.rs` — struct expression lowering

In `lp-shader/lps-frontend/src/lower_expr.rs`:

#### 5a. `Expression::AccessIndex` — struct local

The existing `Expression::AccessIndex` match has arms for
`TypeInner::Vector` / `TypeInner::Matrix` / `TypeInner::Array`
already. Add a `TypeInner::Pointer { base: Struct { .. } }` arm
when the base resolves to a slot-backed struct local:

- Resolve the local: `Expression::LocalVariable(lv)` from `base`.
- Look up `aggregate_map[lv]` of struct kind.
- Call `lower_struct::load_struct_member_to_vregs(ctx, info, *index as usize)`
  and return the resulting `VRegVec`.

#### 5b. `Expression::AccessIndex` — struct via `inout`/`out` param pointer

If the base is `FunctionArgument(i)` and `pointer_args.get(&i)` returns
a `Handle<Type>` whose `aggregate_layout(...)` is `Struct { .. }`:

- Get the param-base `VReg` from `arg_vregs_for(i)[0]`.
- Look up `members[index].byte_offset` and `ir_tys`.
- Emit per-IR-component `Load { dst, base: param_base, offset:
member.byte_offset + j*4 }`.

#### 5c. `Expression::Load` — struct local

Today `Load(LocalVariable)` for arrays already errors via
`UnsupportedExpression` (consumers must dispatch on `aggregate_map`).
Mirror that for struct locals. Confirm by reading any existing array
example in `lower_expr.rs` for the precise error wording style.

#### 5d. `Expression::Compose { ty: Struct }`

When the expression result is consumed at a known destination
(`lower_array_initializer`-style call, or assigned-to local), the
caller passes the destination to `store_lps_value_into_slot` and
phase 04's struct arm handles it. When `Compose` is called via
`ensure_expr_vec` _with no destination_ (e.g. directly returned, or
passed to a function — those are phase 05), error explicitly:

```rust
Expression::Compose { ty, .. } if matches!(
    &ctx.module.types[*ty].inner, TypeInner::Struct { .. }
) => Err(LowerError::UnsupportedExpression(String::from(
    "struct Compose without destination slot — phase 05 routes call args",
))),
```

This is intentional: any leftover struct-rvalue site without a slot is
a phase-05 thing; we'd rather error than silently fall back.

### 6. `lower_stmt.rs` — struct stores

In `lp-shader/lps-frontend/src/lower_stmt.rs`, extend `Statement::Store`:

#### 6a. Whole-struct assignment to a local

`Statement::Store { pointer = LocalVariable(lv), value }` where
`aggregate_map[lv]` is struct-kind:

```rust
let info = ctx.aggregate_map.get(&lv).cloned().expect("struct local");
let lps_ty = naga_types::naga_type_handle_to_lps(ctx.module, info.naga_ty)?;
let base = aggregate_storage_base_vreg(ctx, &info.slot)?;
store_lps_value_into_slot(ctx, base, 0, &lps_ty, value)?;
```

The Memcpy fast path inside the primitive collapses this to one
`LpirOp::Memcpy` when `value` resolves to a slot-backed struct of
matching layout (e.g. `output = input;`).

#### 6b. Member-store on a struct local

`Statement::Store { pointer = AccessIndex(LocalVariable(lv), m), value }`:

- Resolve `info = aggregate_map[lv]` (struct).
- Get `member = info.layout.struct_members()[m]`.
- Compute `base = aggregate_storage_base_vreg(...)`.
- Lower `value` with `ensure_expr_vec` + `coerce_assignment_vregs` for
  `member.lps_ty`; emit per-IR-component `Store { base, offset:
member.byte_offset + j*4, value }`.

### 7. `lower_access.rs` — member-store through pointer

Extend `lower_access.rs` for `Statement::Store` where `pointer` peels to
`AccessIndex(FunctionArgument(i), m)` and `pointer_args[i]` is a struct
type:

- `param_base = arg_vregs[i][0]`.
- `layout = aggregate_layout(pointee)?.struct_members()`.
- `member = layout[m]`.
- `value` lowered as in 6b; per-IR-component `Store` at
  `param_base + member.byte_offset + j*4`.

(This unblocks `process_circle`'s `output.radius = …` pattern in
`function/param-struct.glsl` even though phase 04 doesn't yet enable
the by-value struct call path for that test as a whole.)

### 8. Print-IR sanity check

Before declaring phase 04 done, dump LPIR for
`struct/constructor-nested.glsl`. With `--debug`, the runner prints
`=== LPIR ===` (and other compile-time sections) to stderr when
compilation succeeds:

```sh
./scripts/filetests.sh --debug struct/constructor-nested.glsl
```

From `lp-shader/`:

```sh
cargo run -p lps-filetests-app --bin lps-filetests-app -- test --debug struct/constructor-nested.glsl
```

Confirm by inspection:

- One slot per struct local of correct std430 size.
- Member stores at correct byte offsets (e.g. `vec3 + float` → offsets
  `0, 4, 8, 12` — `float` first 12 bytes, `vec3` aligned at 0 if
  declared first; double-check against `lower_aggregate_layout`'s test
  `std430_struct_vec3_float`).
- No `Load` of a struct local; member access lowers to
  `Load { base, offset }` directly.

If anything looks off (e.g. fast path not firing on `output = input;`,
or an extra unpack-then-pack), fix before declaring done — the
downstream phases compound on this.

## Validate

From the workspace root:

```sh
just check
```

Then:

```sh
cargo test -p lps-frontend
./scripts/filetests.sh array/
./scripts/filetests.sh function/return-array.glsl
./scripts/filetests.sh function/param-array.glsl
./scripts/filetests.sh struct/
```

**Required:**

- All array-related filetests stay in their pre-phase pass/fail state
  on every default target.
- `struct/*.glsl` tests that don't cross a struct-value function
  boundary now pass on `wasm.q32`, `rv32c.q32`, `rv32n.q32`. Tests
  that _do_ cross such a boundary (struct return, struct value param)
  may still fail — this is the phase 05 boundary.
- `rv32c.q32` and `rv32n.q32` must show identical pass/fail sets.
  Divergence = backend bug to fix in this phase before declaring done.
- No filetest annotations toggled in this phase — phase 06 handles
  `--fix`.

The test runner output may show "unexpected pass" for struct tests now
passing; that's fine, phase 06 cleans annotations.
