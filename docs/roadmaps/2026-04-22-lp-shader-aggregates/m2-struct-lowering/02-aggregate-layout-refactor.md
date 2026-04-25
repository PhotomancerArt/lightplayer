# Phase 02 — `aggregate_layout` + `AggregateInfo` migration (no behaviour change)

**Tags:** sub-agent: yes, parallel: no (foundation for phases 03–05)

## Scope of phase

Pure refactor. Introduce a single `AggregateLayout` query and migrate
`AggregateInfo` to carry it instead of array-only fields. Replace the
five "is this an aggregate?" decision sites with the new query. **No
struct logic** — every code path that recognises `TypeInner::Array`
today continues to recognise only arrays after this phase. All existing
array filetests stay green.

This phase exists so phase 03 (struct lowering) only adds *new* arms —
it does not also have to refactor the array machinery.

### Out of scope

- Anything `TypeInner::Struct`. The `aggregate_layout` query may
  short-circuit `Struct` → `None` for now (or panic — see below).
- Any change to `lower_array.rs` slot-write logic. (That refactor is
  phase 03.)
- Any change to filetests. No `--fix`, no marker toggling.
- Any new module files except where listed below.

## Code organization reminders

- One concept per file; helpers at the bottom; abstract things and
  entry points at the top.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment so it can be found.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. No struct work, no array slot-write refactor.
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix
  them.
- Do **not** disable, skip, or weaken existing tests.
- If something blocks completion (ambiguity, unexpected design issue),
  stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

## Implementation details

### 1. New layout query in `naga_util.rs`

Add to `lp-shader/lps-frontend/src/naga_util.rs`:

```rust
/// Type-level layout for an aggregate Naga type (array or struct), shared by
/// every "is this aggregate, and if so where do its parts live?" decision in
/// the frontend. Returns `None` for non-aggregates.
///
/// For phase 02: the `Struct` arm is intentionally unimplemented; phase 03
/// fills it in. Callers must not be passed struct types yet.
pub(crate) fn aggregate_layout(
    module: &Module,
    ty: Handle<Type>,
) -> Result<Option<AggregateLayout>, LowerError> {
    match &module.types[ty].inner {
        TypeInner::Array { .. } => {
            let (dimensions, leaf_element_ty, leaf_stride) =
                crate::lower_array_multidim::flatten_array_type_shape(module, ty)?;
            let element_count = dimensions
                .iter()
                .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                .ok_or_else(|| {
                    LowerError::Internal(String::from("aggregate_layout: count overflow"))
                })?;
            let (total_size, align) =
                crate::lower_aggregate_layout::aggregate_size_and_align(module, ty)?;
            Ok(Some(AggregateLayout {
                kind: AggregateKind::Array {
                    dimensions,
                    leaf_element_ty,
                    leaf_stride,
                    element_count,
                },
                total_size,
                align,
            }))
        }
        TypeInner::Struct { .. } => Err(LowerError::UnsupportedType(String::from(
            "aggregate_layout: struct support lands in M2 phase 03",
        ))),
        _ => Ok(None),
    }
}

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
    /// Reserved for phase 03.
    #[allow(
        dead_code,
        reason = "phase 03 lands struct lowering and fills this in"
    )]
    Struct,
}
```

### 2. `AggregateInfo` carries the layout

In `lp-shader/lps-frontend/src/lower_ctx.rs`, replace:

```rust
pub(crate) struct AggregateInfo {
    pub slot: AggregateSlot,
    pub dimensions: SmallVec<[u32; 4]>,
    pub leaf_element_ty: Handle<Type>,
    pub leaf_stride: u32,
    pub element_count: u32,
    pub total_size: u32,
}
```

with:

```rust
pub(crate) struct AggregateInfo {
    pub slot: AggregateSlot,
    pub layout: crate::naga_util::AggregateLayout,
}
```

Add accessor shims that preserve current call-site readability for the
**array-only** consumers (`lower_array.rs`, `lower_array_multidim.rs`,
`lower_call.rs`):

```rust
impl AggregateInfo {
    /// Array-only — panics on Struct (phase 03 introduces struct accessors).
    pub fn dimensions(&self) -> &[u32] {
        match &self.layout.kind {
            crate::naga_util::AggregateKind::Array { dimensions, .. } => dimensions,
            crate::naga_util::AggregateKind::Struct => {
                unreachable!("AggregateInfo::dimensions on struct (phase 03)")
            }
        }
    }
    pub fn leaf_element_ty(&self) -> Handle<Type> { /* … */ }
    pub fn leaf_stride(&self) -> u32 { /* … */ }
    pub fn element_count(&self) -> u32 { /* … */ }
    pub fn total_size(&self) -> u32 { self.layout.total_size }
}
```

Then sweep every existing read of `info.dimensions` /
`info.leaf_element_ty` / `info.leaf_stride` / `info.element_count` /
`info.total_size` to use the accessor. (`info.slot` stays a field.)

### 3. Replace the five ABI-decision sites with `aggregate_layout`

#### 3a. `naga_util::func_return_ir_types_with_sret`

Currently:

```rust
let inner = &module.types[h].inner;
if matches!(inner, TypeInner::Array { .. }) {
    let (size, _align) = crate::lower_aggregate_layout::aggregate_size_and_align(module, h)?;
    return Ok(FuncReturnAbi { returns: Vec::new(), sret: Some(IrType::Pointer), sret_size: size });
}
```

Replace with:

```rust
if let Some(layout) = aggregate_layout(module, h)? {
    return Ok(FuncReturnAbi {
        returns: Vec::new(),
        sret: Some(IrType::Pointer),
        sret_size: layout.total_size,
    });
}
```

(For phase 02 the `aggregate_layout` Struct arm errors, which is fine —
no struct returns are reachable yet.)

#### 3b. `LowerCtx::new` — by-value `in` array param arm

The current `TypeInner::Array { .. }` arm in the param loop builds
`PendingInArrayValueArg` from
`crate::lower_aggregate_layout::aggregate_size_and_align` +
`flatten_array_type_shape`. Rewrite to call `aggregate_layout(module,
arg.ty)?`, destructure the `AggregateKind::Array { .. }`, and build the
`PendingInArrayValueArg` from those fields. Behaviour must be identical.

#### 3c. `LowerCtx::new` — local-array allocation arm

Same swap for the local-variable loop's `TypeInner::Array { .. }` arm.
Watch out for the `array_type_has_inferred_dimension` branch — keep the
existing per-shape behaviour (inferred-dim arrays use the local shape,
not the std430 size). Implementation hint: if the type has an inferred
dim, skip `aggregate_layout` and fall back to `flatten_local_array_shape`
exactly as today. (`aggregate_layout` will error on inferred-dim arrays
because `aggregate_size_and_align` cannot handle them; that's fine, do
the inferred-dim check first.)

#### 3d. `LowerCtx::aggregate_info_for_subscript_root` — `Param` arm

Currently:

```rust
if !matches!(self.module.types[pointee].inner, TypeInner::Array { .. }) {
    return Ok(None);
}
let (dimensions, leaf_ty, leaf_stride) = …;
let (total_size, _align) = …;
Ok(Some(AggregateInfo { slot: AggregateSlot::Param(arg_i), … }))
```

Replace with `aggregate_layout(self.module, pointee)?` → if `Some(layout)`
build `AggregateInfo { slot: AggregateSlot::Param(arg_i), layout }`,
else `Ok(None)`.

#### 3e. `lower_call::lower_user_call` — arg loop's `TypeInner::Array` branch

```rust
} else if matches!(callee_inner, TypeInner::Array { .. }) {
    let lv = call_arg_array_local(ctx, arg_h)?;
    let info = ctx.aggregate_map.get(&lv).cloned().ok_or_else(|| …)?;
    let addr = aggregate_storage_base_vreg(ctx, &info.slot)?;
    arg_vs.push(addr);
}
```

Replace with `aggregate_layout(ctx.module, callee_arg.ty)?.is_some()`
gate. Body unchanged for phase 02.

#### 3f. `lower_call::record_call_result_aggregate` and `write_aggregate_return_into_sret`

Both currently call `flatten_array_type_shape` + `aggregate_size_and_align`
manually to build an `AggregateInfo`. Replace each with
`aggregate_layout(ctx.module, naga_ret_ty)?` →
`AggregateInfo { slot, layout }`. The `Compose | ZeroValue` branch in
`write_aggregate_return_into_sret` keeps its array-only call into
`crate::lower_array::zero_fill_array` / `lower_array_initializer` — that
path is array-only by construction in phase 02.

### 4. Don't introduce dead code

`naga_util::array_type_flat_components_for_value_coercions` and
`array_ty_pointer_arg_ir_type` keep their existing roles. Don't delete
them. Don't add `cfg(test)` gating they don't already have.

## Validate

From the workspace root:

```sh
just check
cargo test -p lps-frontend --no-run
cargo test -p lps-frontend
```

Then targeted filetests (from workspace root — same runner as
`just test-filetests`):

```sh
./scripts/glsl-filetests.sh array/
./scripts/glsl-filetests.sh function/return-array.glsl
./scripts/glsl-filetests.sh function/param-array.glsl
```

Equivalent from `lp-shader/`:

```sh
cargo run -p lps-filetests-app --bin lps-filetests-app -- test array/
cargo run -p lps-filetests-app --bin lps-filetests-app -- test function/return-array.glsl
cargo run -p lps-filetests-app --bin lps-filetests-app -- test function/param-array.glsl
```

All array-related filetests must remain in their pre-phase pass/fail
state on every default target (`wasm.q32`, `rv32c.q32`, `rv32n.q32`).
No new failures, no new passes.

If any pass changes — investigate before declaring success. The phase
is **behaviour-preserving** and any delta is a refactor bug.
