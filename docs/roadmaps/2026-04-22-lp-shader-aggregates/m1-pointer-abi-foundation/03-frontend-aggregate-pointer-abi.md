# P3 — Frontend: aggregate generalisation + pointer ABI

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md` (Q2, Q7, Q9).
Depends on: P1 (LPIR sret marker), P2 (layout authority).
Parallel with: nothing — touches the central frontend lowering.

## Scope of phase

Migrate the frontend to the unified pass-by-pointer aggregate ABI. After
this phase, every aggregate (today: arrays) crosses function boundaries
as a single `IrType::Pointer` arg, and aggregate returns use the LPIR
sret marker introduced in P1.

Concretely:

- Rename `ArrayInfo` → `AggregateInfo`, `ArraySlot` → `AggregateSlot`.
- `LowerCtx::new` classifies aggregate `in` parameters as a single
  `IrType::Pointer` param (instead of many flat scalar params) and emits
  a `Memcpy` from the param pointer into a local slot at function entry.
- For functions whose return type is an aggregate, allocate the sret
  pointer via `FunctionBuilder::add_sret_param` *before* user params,
  and lower `Statement::Return(value)` as
  `Memcpy(sret_arg, value_addr, size) + push_return(&[])`.
- Naga signature helpers swap their flat-array helpers for pointer-arg /
  sret-aware variants:
  - `array_type_flat_ir_types` → `array_ty_pointer_arg_ir_type`
    (returns `IrType::Pointer`).
  - `func_return_ir_types` → `func_return_ir_types_with_sret`
    (returns `(returns: Vec<IrType>, sret_ptr: Option<IrType::Pointer>)`).
- Extract the call-lowering code (`lower_user_call` and helpers) from
  `lower_stmt.rs` into a new `lower_call.rs`. The new ABI is implemented
  there:
  - Aggregate args: push the slot address (via `SlotAddr`); no flatten.
  - Aggregate returns: allocate a dest slot, push its address as the
    hidden first arg, do not push result VRegs into `push_call`. Mark
    the call result so subsequent reads load from the slot.
- Delete `lower_array::store_array_from_flat_vregs` and
  `lower_array::load_array_flat_vregs_for_call`.

**Out of scope:**

- Backend codegen (P4–P7).
- Any host marshalling (P6, P7, P8).
- New aggregate types beyond arrays (M2+).
- Read-only `in` optimisation (M5).

This phase will leave the workspace **broken at the backend** until
P4–P7 land — that is expected and called out in P10. Compile-time it
must still pass `cargo check -p lps-frontend`; behavioural filetests
will fail until backends catch up.

## Code organization reminders

- Pull `lower_user_call` and helpers (`call_arg_array_local`,
  `call_arg_pointer_local`, `lower_lpfn`-call-related glue if it's purely
  call-site code) out of `lower_stmt.rs` into a new
  `lp-shader/lps-frontend/src/lower_call.rs`. Keep `lower_stmt.rs`'s
  `Statement::Call` arm thin: dispatch into `lower_call.rs`.
- Place new helpers (Memcpy-on-entry for aggregate `in`, sret-Memcpy for
  aggregate returns, dest-slot materialisation for aggregate call
  results) in `lower_call.rs` and `lower_array.rs` as appropriate.
- Don't leave behind dead helpers — delete `store_array_from_flat_vregs`
  and `load_array_flat_vregs_for_call` and any private helpers they
  used that now have no other callers.
- Mark anything genuinely temporary with a `// TODO(M1):` comment so it
  can be found in P10.

## Sub-agent reminders

- Do **not** commit.
- Stay strictly within `lp-shader/lps-frontend/`. Do not touch
  `lpir/`, `lpvm/`, `lpvm-*/`, or filetest `CHECK:` lines (those are
  P9). You may touch `lp-shader/lps-frontend/src/tests/` to add unit
  coverage.
- This is the largest phase of M1. **Do not expand scope.** If you find
  yourself wanting to fix something orthogonal (e.g. a clippy lint
  somewhere unrelated, a refactor that "would be nice"), stop and add
  a TODO instead.
- Do **not** suppress warnings or add `#[allow(...)]`.
- Do **not** weaken or `#[ignore]` tests. Frontend unit tests must
  remain green; new ones can be added.
- If the workspace stops compiling because backends now expect the old
  flat-array signatures from `naga_util`, that's expected — leave them
  broken; P4–P7 will fix the backends. **But** `cargo check -p lps-frontend`
  must succeed. Do not push a state where `lps-frontend` itself
  doesn't compile.
- If you uncover a design ambiguity (e.g. how do we handle a corner
  case in `Statement::Return` where the value is computed in flat
  VRegs rather than already in a slot?), stop and report rather than
  improvising.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. Renames

`ArrayInfo` → `AggregateInfo`, `ArraySlot` → `AggregateSlot`,
`array_map` → `aggregate_map`, `array_info_for_subscript_root` →
`aggregate_info_for_subscript_root`, `array_storage_base_vreg` →
`aggregate_storage_base_vreg`. Update all references.

```rust
// lower_ctx.rs
pub(crate) struct AggregateInfo {
    pub slot: AggregateSlot,
    pub dimensions: SmallVec<[u32; 4]>,
    pub leaf_element_ty: Handle<Type>,
    pub leaf_stride: u32,
    pub element_count: u32,
    /// Total bytes (= aggregate_size_and_align(...).0). Filled from
    /// lower_aggregate_layout for consistency.
    pub total_size: u32,
}

pub(crate) enum AggregateSlot {
    Local(SlotId),
    /// Function argument index (0-based across user params); the corresponding
    /// LPIR param is an `IrType::Pointer` to caller's buffer (after Memcpy
    /// into a local slot, this becomes Local(...) — see #2 below).
    Param(usize),
}
```

For M1, after the entry-Memcpy (#2), every aggregate `in` param is
*also* slot-backed locally — so the `Param` arm largely disappears for
local use. Keep `AggregateSlot::Param` only if there's a code path that
still needs it (e.g. for diagnostics). If it's dead after #2, delete
the variant.

### 2. Aggregate `in` params: pointer + entry Memcpy

In `lower_ctx.rs::LowerCtx::new`, replace the `TypeInner::Array { .. }`
arm of the parameter classification:

**Before** (sketch):

```rust
TypeInner::Array { .. } => {
    let ir_tys = array_ty_flat_ir_types(module, arg.ty)?;
    for ty in &ir_tys { fb.add_param(*ty); }
    // ...
}
```

**After**:

```rust
TypeInner::Array { .. } => {
    // Pass aggregate by pointer: one Pointer param + local slot + entry Memcpy.
    let (size, align) = lower_aggregate_layout::aggregate_size_and_align(module, arg.ty)?;
    let _ = align; // align is informational here; slot byte size is what we need.
    let param_ptr = fb.add_param(IrType::Pointer);
    let local_slot = fb.alloc_slot(size);
    let local_addr = fb.alloc_vreg(IrType::Pointer);
    fb.push(LpirOp::SlotAddr { dst: local_addr, slot: local_slot });
    fb.push(LpirOp::Memcpy {
        dst_addr: local_addr,
        src_addr: param_ptr,
        size,
    });
    // Record both the slot (so AccessIndex / array reads resolve to local memory)
    // and the dimensions/leaf-stride for indexed loads.
    let (dimensions, leaf_ty, leaf_stride) =
        crate::lower_array_multidim::flatten_array_type_shape(module, arg.ty)?;
    let element_count = dimensions.iter().product();
    aggregate_map.insert(/* key for this arg */ ..., AggregateInfo {
        slot: AggregateSlot::Local(local_slot),
        dimensions,
        leaf_element_ty: leaf_ty,
        leaf_stride,
        element_count,
        total_size: size,
    });
}
```

Key detail: the **key** in `aggregate_map` was `Handle<LocalVariable>`
under the old design. With pointer args, the param has *no*
`LocalVariable` — its body refers to `FunctionArgument(i)` and (Naga
sometimes) inserts a `LocalVariable` mirror. Look at how
`scan_param_argument_indices` (`lower_ctx.rs:357`) currently aliases
`Store(LocalVariable, FunctionArgument)` and reuse that mapping: the
local mirror is the key, the slot is `local_slot`.

If the Naga-mirror local is *not* always present for `in` aggregate
args, fall back to a parallel `argument_aggregate_map: HashMap<usize,
AggregateInfo>` and have `aggregate_info_for_subscript_root` consult
both. Whichever way you pick, document it in a doc-comment.

Do **not** keep the old "many flat scalar args + scan_param +
store_array_from_flat_vregs at entry" path. Delete it.

### 3. Drop dead helpers

`lower_array.rs`:

- Delete `store_array_from_flat_vregs` (around line 592).
- Delete `load_array_flat_vregs_for_call` (around line 627).
- Delete any of their private helpers that have no other callers.

`naga_util.rs`:

- Replace `array_type_flat_ir_types(module, ty) -> Vec<IrType>` with:

  ```rust
  pub(crate) fn array_ty_pointer_arg_ir_type(
      _module: &Module,
      _ty: Handle<Type>,
  ) -> Result<IrType, LowerError> {
      Ok(IrType::Pointer)
  }
  ```

  Plus update every call site to consume `IrType::Pointer` (only one
  arg slot, not many).

  If `array_type_flat_ir_types` is referenced from elsewhere (unit
  tests, debug helpers), audit each caller and switch them too — or
  delete dead callers.

- Replace `func_return_ir_types(...) -> Vec<IrType>` with:

  ```rust
  pub(crate) struct FuncReturnAbi {
      /// LPIR `return_types`. Empty when sret is set.
      pub returns: Vec<IrType>,
      /// `Some(IrType::Pointer)` when the function returns an aggregate.
      pub sret: Option<IrType>,
      /// Size (bytes) of the sret destination buffer. Zero when no sret.
      pub sret_size: u32,
  }

  pub(crate) fn func_return_ir_types_with_sret(
      module: &Module,
      ret_ty: Option<Handle<Type>>,
  ) -> Result<FuncReturnAbi, LowerError> {
      // For TypeInner::Array (and later Struct), produce FuncReturnAbi {
      //   returns: vec![],
      //   sret: Some(IrType::Pointer),
      //   sret_size: aggregate_size_and_align(module, ty).0,
      // }
      // Otherwise: returns = naga_type_to_ir_types(...), no sret.
  }
  ```

  Audit every call site; some live in `lower.rs::lower_function` for
  building the function's signature, and some in `lower_call.rs` for
  reading the callee's signature.

### 4. Aggregate returns — sret path in `lower.rs`

In `lower_function` (search `FunctionBuilder::new` calls in
`lower.rs`), determine the function's return ABI via
`func_return_ir_types_with_sret`. When `sret` is set:

1. **Before** any user-param `add_param` calls, call `add_sret_param`.
   This places the sret pointer at `VReg(vmctx + 1)` and shifts user
   params to `VReg(vmctx + 2 + i)`.
2. Stash the sret VReg in `LowerCtx` so `Statement::Return` can find it:

   ```rust
   // lower_ctx.rs
   pub(crate) struct LowerCtx<'a> {
       // ...
       pub sret: Option<SretCtx>,
   }

   pub(crate) struct SretCtx {
       pub addr: VReg,        // = func.sret_arg.unwrap()
       pub size: u32,         // = aggregate size in bytes
   }
   ```

3. Pass `return_types: &[]` to `FunctionBuilder::new` when sret is set.

In `lower_stmt.rs::Statement::Return`:

```rust
Statement::Return { value } => match value {
    Some(expr) => {
        let mut vs = ctx.ensure_expr_vec(*expr)?;
        if let Some(res) = &ctx.func.result {
            let dst_inner = &ctx.module.types[res.ty].inner;
            vs = coerce_assignment_vregs(ctx, Some(res.ty), dst_inner, *expr, vs)?;
        }
        if let Some(sret) = ctx.sret.clone() {
            // Aggregate return: write components into the sret buffer
            // and ret void. The simplest universal lowering is to find
            // (or materialise) a slot for the result, store the components
            // there, then Memcpy(sret.addr, slot_addr, sret.size).
            crate::lower_call::write_aggregate_return_into_sret(ctx, *expr, vs, &sret)?;
            ctx.fb.push_return(&[]);
        } else {
            ctx.fb.push_return(&vs);
        }
        Ok(())
    }
    None => { ctx.fb.push_return(&[]); Ok(()) }
},
```

`write_aggregate_return_into_sret` lives in `lower_call.rs` and handles
both shapes:

- **Result is already in a slot** (e.g. the returned expression is a
  local array variable). Resolve its base address via
  `aggregate_storage_base_vreg`, then `Memcpy(sret.addr, base, size)`.
- **Result is in flat VRegs** (e.g. a literal Compose, or some scalar /
  vec / mat being returned by an `out`-aggregate function — note: M1
  only handles aggregate returns, scalar returns stay flat). For an
  aggregate this shouldn't happen for arrays today: arrays in GLSL only
  return as variable references. Add a clear `LowerError::Internal` for
  the "flat VReg aggregate return" case so M2 can detect when struct
  literals start hitting this path.

### 5. Caller side — `lower_call.rs`

Move `lower_user_call` (currently `lower_stmt.rs:504-633`) and its
helpers into a new `lp-shader/lps-frontend/src/lower_call.rs`. Replace
the two aggregate arms:

**Aggregate args (replaces `TypeInner::Array { .. }` arm with the flat
load):**

```rust
} else if matches!(&ctx.module.types[callee_arg.ty].inner, TypeInner::Array { .. }) {
    let lv = call_arg_array_local(ctx, arg_h)?;
    let info = ctx.aggregate_map.get(&lv).cloned().ok_or_else(|| {
        LowerError::UnsupportedExpression(String::from(
            "aggregate call argument: not a stack-slot aggregate",
        ))
    })?;
    let addr = crate::lower_array::aggregate_storage_base_vreg(ctx, &info.slot)?;
    arg_vs.push(addr);
}
```

**Aggregate return (replaces the `array_type_flat_ir_types` materialisation):**

```rust
let mut result_vs = Vec::new();
let mut sret_dest: Option<(SlotId, u32)> = None;

if let Some(res_h) = result {
    let res_ty = f.result.as_ref().ok_or_else(|| ...)?;
    let abi = crate::naga_util::func_return_ir_types_with_sret(ctx.module, Some(res_ty.ty))?;
    if abi.sret.is_some() {
        // Allocate dest slot; pass its address as the hidden first arg
        // (immediately after vmctx in arg_vs).
        let slot = ctx.fb.alloc_slot(abi.sret_size);
        let addr = ctx.fb.alloc_vreg(IrType::Pointer);
        ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });
        // Insert sret immediately after vmctx (which is arg_vs[0]).
        // arg_vs currently starts as [vmctx, user_args...]; insert at 1.
        arg_vs.insert(1, addr);
        sret_dest = Some((slot, abi.sret_size));
        // Cache the result expression as "lives in slot" so subsequent
        // reads go through indexed Loads (record an AggregateInfo for it).
        record_call_result_aggregate(ctx, res_h, res_ty.ty, slot)?;
    } else {
        // Scalar/vec/mat return: original flat-VRegs path.
        let inner = &ctx.module.types[res_ty.ty].inner;
        let ir_tys = naga_type_to_ir_types(inner)?.to_vec();
        let mut vregs = VRegVec::new();
        for ty in &ir_tys {
            let v = ctx.fb.alloc_vreg(*ty);
            vregs.push(v);
            result_vs.push(v);
        }
        if let Some(slot) = ctx.expr_cache.get_mut(res_h.index()) {
            *slot = Some(vregs);
        }
    }
}

ctx.fb.push_call(callee_ref, &arg_vs, &result_vs);
// ... (existing inout copybacks unchanged) ...
```

`record_call_result_aggregate` creates a fresh `AggregateInfo` with
`slot = AggregateSlot::Local(slot)` and inserts it into a *result*
aggregate map (parallel to `aggregate_map` which is keyed by
`Handle<LocalVariable>`). When the body subsequently reads `res_h[i]`,
the read path resolves to `Load(slot_addr + i*stride)`. Implementation
detail: simplest is to extend `aggregate_map` to also key on
`Handle<Expression>` for call results; alternatively maintain a sibling
`call_result_aggregates: BTreeMap<Handle<Expression>, AggregateInfo>`
and have the readers consult both.

If implementing this turns out to need more invasive expression-cache
plumbing than expected, **stop and report.**

### 6. Arg ordering invariant

Confirm and document: for any callee, the LPIR `Call.args` operand
order is:

```
[vmctx, sret?, user_arg_0, user_arg_1, ...]
```

`sret?` is present iff the callee's `IrFunction::sret_arg.is_some()`
(local) or `ImportDecl::sret == true` (import). Add a comment to
`lower_call.rs::lower_user_call` and the `Call` op's docs in
`lpir_op.rs`. (Editing `lpir_op.rs` is a one-line doc update; that's
within the spirit of this phase, even though it's outside `lps-frontend/`.
Acceptable. If you'd rather not, leave a TODO and the LPIR doc gets a
nudge in P10.)

### 7. Tests

Add lightweight unit tests in `lp-shader/lps-frontend/src/tests.rs`
(or appropriate sibling) that lower a small GLSL function with:

- An `in float[4]` parameter — confirm exactly one IR param of
  `IrType::Pointer`, and a `Memcpy` is the first op of the body.
- A function returning `float[4]` — confirm `IrFunction::sret_arg ==
  Some(VReg(1))`, `return_types.is_empty()`, `param_count == 0`,
  body's `Return` has `values.count == 0`.
- A call site that calls a function returning `float[4]` — confirm the
  call's `args` slice begins `[vmctx, sret_dest_addr, ...]`.

These tests verify the IR shape only; behavioural tests come in P9.

## Validate

```
cargo check -p lps-frontend
cargo test  -p lps-frontend
```

Workspace-wide compilation (`just check`) is **expected to fail** until
P4–P7 land — backends still consume the old flat-array signature
helpers. That is acceptable for this phase; do not silence it by
gutting `naga_util`'s deprecated names.

If `cargo check -p lps-frontend` itself fails, fix it before reporting
back.

## Done when

- `ArrayInfo`/`ArraySlot` renamed to `AggregateInfo`/`AggregateSlot`.
- `LowerCtx::new` classifies aggregate `in` as Pointer + slot + Memcpy.
- `lower_function` (in `lower.rs`) calls `add_sret_param` when the
  return is aggregate, and stashes `SretCtx` in `LowerCtx`.
- `Statement::Return` uses `SretCtx` when present.
- `lower_user_call` extracted into `lower_call.rs`; aggregate args push
  slot address; aggregate returns allocate dest slot and pass as first
  arg.
- `store_array_from_flat_vregs` and `load_array_flat_vregs_for_call`
  deleted.
- `array_type_flat_ir_types` replaced; `func_return_ir_types` replaced
  with `func_return_ir_types_with_sret`.
- New unit tests pass.
- `cargo check -p lps-frontend` and `cargo test -p lps-frontend` are
  green.
- No new `#[allow(...)]`; no `#[ignore]`d tests.
