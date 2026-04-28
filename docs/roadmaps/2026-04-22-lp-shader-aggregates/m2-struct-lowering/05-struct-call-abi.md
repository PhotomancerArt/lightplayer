# Phase 05 — Struct call ABI (args + sret return)

**Tags:** sub-agent: yes, parallel: no (depends on phase 04)

## Scope of phase

Land struct support across function-call boundaries:

- By-value `in` struct arg → caller passes pointer to its slot.
- `inout`/`out` struct arg → already works for arrays via
  `pointer_args`; verify it works for structs after phase 04 enabled
  the `aggregate_map` entries on the callee side.
- Struct return → sret. Caller allocates dest slot, passes pointer as
  hidden first arg; callee writes through it via existing
  `write_aggregate_return_into_sret` (now generalised over arbitrary
  aggregates).
- Struct rvalue (Compose / CallResult) appearing as a direct call arg →
  materialise into a temp slot via `lower_struct::
materialise_aggregate_rvalue_to_temp_slot`, pass that slot's pointer.

Acceptance: `function/param-struct.glsl` and `function/return-struct.glsl`
pass on `wasm.q32`, `rv32c.q32`, `rv32n.q32`.

### Out of scope

- `--fix` of any filetest annotations (phase 06).
- Any change to `LpvmDataQ32` host-side marshalling. Structs already
  flow through pointer ABI; the M1 work made `LpvmDataQ32::from_value`
  / `to_value` recurse through structs.
- Read-only-`in` optimisation (M5).

## Code organization reminders

- One concept per file.
- Helpers at the bottom; abstract entry points at the top.
- Group related functionality.
- Any temporary code must have a `TODO` comment.

## Sub-agent reminders

- Do **not** commit. Plan commits at the end as a single unit.
- Do **not** expand scope into M3+ (arrays of structs, etc.).
- Do **not** suppress warnings — fix them.
- Do **not** weaken or skip existing tests.
- Stop and report on ambiguity rather than improvising.

## Implementation details

### 1. `lower_call.rs` — argument loop accepts structs

In `lp-shader/lps-frontend/src/lower_call.rs::lower_user_call`'s arg loop:

#### 1a. Pointer (`inout`/`out`) struct args

The existing `TypeInner::Pointer { base, .. }` branch handles
`call_arg_pointer_local` + `aggregate_map.get(&lv)` and pushes the
slot's base address. Phase 04 added struct entries to `aggregate_map`,
so this branch _should_ now Just Work for struct `inout`/`out`. Verify
by reading the branch and confirming nothing assumes the pointee is
array-shaped. If it does (e.g. unconditionally calls a flatten or
size-by-element-count helper), fix.

#### 1b. By-value `in` aggregate args (array OR struct)

Generalise this branch:

```rust
} else if matches!(callee_inner, TypeInner::Array { .. }) {
    let lv = call_arg_array_local(ctx, arg_h)?;
    let info = ctx.aggregate_map.get(&lv).cloned().ok_or_else(|| …)?;
    let addr = aggregate_storage_base_vreg(ctx, &info.slot)?;
    arg_vs.push(addr);
}
```

into:

```rust
} else if aggregate_layout(ctx.module, callee_arg.ty)?.is_some() {
    // Two cases:
    //   (1) caller-side argument is a known slot-backed aggregate
    //       (LocalVariable / Load(LocalVariable) / CallResult).
    //   (2) caller-side argument is an rvalue (Compose / etc.) — we
    //       materialise it into a temp slot first.
    let addr = aggregate_arg_pointer(ctx, arg_h, callee_arg.ty)?;
    arg_vs.push(addr);
}
```

`aggregate_arg_pointer` (new helper, in `lower_call.rs`):

- Peel `arg_h` for `LocalVariable(lv) | Load { LocalVariable(lv) }`
  with `aggregate_map.get(lv).is_some()` → return
  `aggregate_storage_base_vreg(ctx, &info.slot)`.
- Peel for `CallResult(_)` with `call_result_aggregates.get(&arg_h)` →
  same.
- Otherwise → call `lower_struct::materialise_aggregate_rvalue_to_temp_slot`
  to allocate a fresh slot, materialise (via the slot-write primitive),
  and return the slot's base. Per Q5, no slot reuse — every
  rvalue-as-arg call gets a fresh slot.

Generalise `call_arg_array_local` into `call_arg_aggregate_local` (or
inline the peel into `aggregate_arg_pointer`). The error wording
should be aggregate-agnostic: "aggregate call argument: not a stack-slot
aggregate" rather than "array call argument".

### 2. `lower_call.rs` — return ABI accepts structs

`record_call_result_aggregate` and `write_aggregate_return_into_sret`
already use `aggregate_layout` (phase 02 refactor). They must continue
to work for struct return types. Confirm:

- `record_call_result_aggregate`: only allocates a slot, builds
  `AggregateInfo { slot, layout, naga_ty }`, registers in
  `call_result_aggregates`. No array-specific code; should already work.
- `write_aggregate_return_into_sret`: the `LocalVariable` /
  `Load(LocalVariable)` / `CallResult` arms are aggregate-agnostic
  (just `Memcpy` the slot to `sret.addr`). The `Compose | ZeroValue`
  arm currently hard-codes `flatten_array_type_shape` +
  `crate::lower_array::lower_array_initializer` /
  `zero_fill_array`. Generalise:

  ```rust
  E::Compose { .. } | E::ZeroValue(_) => {
      let res_ty = ctx.func.result.as_ref().expect("...").ty;
      let layout = aggregate_layout(ctx.module, res_ty)?
          .ok_or_else(|| LowerError::Internal(String::from(
              "sret without aggregate result type")))?;
      let temp = ctx.fb.alloc_slot(layout.total_size);
      let taddr = ctx.fb.alloc_vreg(IrType::Pointer);
      ctx.fb.push(LpirOp::SlotAddr { dst: taddr, slot: temp });
      let lps_ty = naga_types::naga_type_handle_to_lps(ctx.module, res_ty)?;
      crate::lower_aggregate_write::store_lps_value_into_slot(
          ctx, taddr, 0, &lps_ty, value_expr,
      )?;
      ctx.fb.push(LpirOp::Memcpy {
          dst_addr: sret.addr, src_addr: taddr, size: sret.size,
      });
      return Ok(());
  }
  ```

  This collapses both array-init and struct-Compose paths through the
  same primitive.

The "missing slot" error path at the bottom of
`write_aggregate_return_into_sret` no longer mentions M2 specifically
— update the message to:

```rust
"cannot lower aggregate return expression for sret"
```

### 3. `naga_util::func_return_ir_types_with_sret` — struct returns

Phase 02 already routes both Array and Struct (via `aggregate_layout`)
to sret. Phase 04 enabled `aggregate_layout`'s Struct arm. Confirm by
reading: a struct-returning function (e.g. `Point2D get_origin()` from
`function/return-struct.glsl`) builds the LPIR function with no
returns and `sret_arg = Some(IrType::Pointer)` of the right size.

### 4. `lower_expr::Compose { ty: Struct }` — relaxation for direct call args

Phase 04 erred on struct-Compose without a destination slot. With
phase 05's `aggregate_arg_pointer` materialising via
`materialise_aggregate_rvalue_to_temp_slot`, the path that previously
called `ensure_expr_vec` on a struct Compose is gone (call sites now
go through `aggregate_arg_pointer` first). Verify by reading whether
any other call path still routes through `ensure_expr_vec` for a
struct-typed expression. If it does (e.g. some statement form), wire
that path to `materialise_aggregate_rvalue_to_temp_slot` too.

### 5. `LowerCtx::new` Memcpy-from-arg-pointer for struct value `in`

Phase 04 added the param-loop arm that allocates a local slot and
emits the Memcpy from the pointer arg. Verify by reading the IR for
one struct param test:

```sh
./scripts/filetests.sh --debug function/param-struct.glsl
```

Expected: `circle_area(Circle c) { … }` callee starts with one
`SlotAddr` for `c`'s local slot, one `Memcpy` from the param pointer,
then the body uses the local slot for member access. No flat-vreg
unpack of `c` anywhere.

### 6. Caller-side flow check for nested struct returns

The `test_param_struct_return` shape:

```glsl
Color test_param_struct_return() {
    Color red = Color(...);
    Color blue = Color(...);
    return blend_colors(red, blue, 0.5);   // ← inner sret
}
// outer function also sret
```

Walk through with the print-IR output:

- Outer function: 1 sret arg, 2 struct slots (`red`, `blue`).
- Inner call `blend_colors(red, blue, 0.5)`:
  - Caller allocates _call result slot_ for the inner call's sret dest.
  - Pushes `[vmctx, &call_result_slot, &red_slot, &blue_slot, factor]`.
- `Statement::Return { value: Some(call_result_h) }`:
  - `write_aggregate_return_into_sret` sees `CallResult`, looks up
    `call_result_aggregates`, emits one `Memcpy` from
    `&call_result_slot` to outer's `sret.addr`.

Confirm this matches.

## Validate

From the workspace root:

```sh
just check
```

Then targeted:

```sh
cargo test -p lps-frontend
./scripts/filetests.sh struct/
./scripts/filetests.sh function/param-struct.glsl
./scripts/filetests.sh function/return-struct.glsl
./scripts/filetests.sh function/return-array.glsl
./scripts/filetests.sh function/param-array.glsl
./scripts/filetests.sh array/
```

**Required:**

- All `function/param-struct.glsl`, `function/return-struct.glsl`, and
  `struct/*.glsl` tests pass on `wasm.q32`, `rv32c.q32`, `rv32n.q32`.
  Tests that previously had `// @unimplemented(wasm.q32 | rv32c.q32 |
rv32n.q32)` will now show "unexpected pass" — that's expected,
  phase 06 toggles the markers.
- `rv32c.q32` and `rv32n.q32` must show identical pass/fail sets on the
  full struct corpus. Divergence = backend bug to fix in this phase.
- All array-related filetests remain in their pre-phase pass/fail state.

If any backend bug surfaces and is **clearly orthogonal** to struct
lowering (e.g. a pre-existing rv32 codegen issue surfaced by a struct
field that happens to be `mat4`), per Q10-i β: file the issue and
re-mark the specific test with `// TODO(bug-N): <reason>`. Default
behaviour is **fix in M2**.
