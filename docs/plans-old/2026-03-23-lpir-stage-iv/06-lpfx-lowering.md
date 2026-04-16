# Phase 6: LPFX Lowering

## Scope

Implement `lower_lpfx.rs` — detection of LPFX calls, creation of
`@lpfx::name` imports, and out-parameter ABI handling via LPIR slots.
Remove the `todo!()` stub in `lower_stmt.rs` for LPFX calls.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.

## Background

LPFX functions are Naga functions whose names start with `lpfx_`. They
are declared in `lpfx_prologue.glsl` (included by `compile()`). The
Naga module contains these as regular functions with bodies that are
empty stubs — the real implementations live in `lps-builtins`.

In the WASM emitter, LPFX calls go through `lps-builtin-ids` to
resolve to a `BuiltinId`, then emit a WASM import call. The LPIR
lowering is similar but float-mode-unaware: we create `@lpfx::name`
imports and let the downstream emitter decide how to dispatch them.

Key difference from std.math: some LPFX functions return vectors via
out-pointers. In LPIR, these become slot-based out-params.

## Implementation Details

### `lower_lpfx.rs` — module-level collection

```rust
pub(crate) fn collect_lpfx_imports(
    module: &naga::Module,
    functions: &[(Handle<Function>, FunctionInfo)],
) -> Vec<LpfxImport>

pub(crate) struct LpfxImport {
    pub callee_handle: Handle<Function>,
    pub name: String,           // e.g. "lpfx_hash1"
    pub param_types: Vec<IrType>,
    pub return_types: Vec<IrType>,
    pub has_out_params: bool,
}
```

Walk all user function bodies recursively (like the WASM emitter's
`collect_lpfx_builtin_ids`). For each `Statement::Call` where the callee
name starts with `lpfx_`:

- Inspect callee argument types. Pointer-typed args are out-parameters.
- Scalar args → `IrType::F32` or `IrType::I32`.
- The LPIR import signature is the **scalarized** version:
    - Vector args become N scalar args (vec2 → 2x F32, vec3 → 3x F32, etc.)
    - Out-pointer args become a single `I32` (slot address).
- Return type: scalarized (scalar → 1 return, vector return via out-ptr → 0 returns).

Collect unique LPFX functions (by callee handle). Create one
`ImportDecl` per unique LPFX function in the module.

### Module-level wiring — `lower.rs`

Before lowering functions:

1. Call `collect_lpfx_imports()`.
2. For each `LpfxImport`:
    - `mb.add_import(ImportDecl { module_name: "lpfx", func_name: name, ... })`
    - Store the mapping: `lpfx_handle → CalleeRef`.
3. Pass this mapping into each function's `LowerCtx`.

### Per-call lowering — `lower_lpfx.rs`

```rust
pub(crate) fn lower_lpfx_call(
    ctx: &mut LowerCtx,
    callee_handle: Handle<Function>,
    arguments: &[Handle<Expression>],
    result: Option<Handle<Expression>>,
) -> Result<(), LowerError>
```

Steps:

1. Look up the LPFX import's `CalleeRef` from `ctx.lpfx_map`.
2. For each argument:
    - If the callee declares this as a pointer-typed arg (out-param):
        - Allocate a slot: `ctx.fb.alloc_slot(N * 4)` where N is the
          number of scalar components in the pointed-to type.
        - Emit `SlotAddr { dst: addr_vreg, slot }`.
        - Add `addr_vreg` to the call args.
        - Record `(slot, N)` in a local `out_params` list for post-call reads.
    - Else (value arg):
        - Lower the expression via `ctx.ensure_expr(arg)`.
        - Add the VReg to call args.
        - (For vector args in the future: would need to scalarize. For now,
          scalar-only scope means this is a single VReg.)
3. Determine result VRegs:
    - If the callee returns a scalar: allocate 1 result VReg.
    - If the callee returns void but has out-params: 0 result VRegs.
4. Emit `ctx.fb.push_call(callee_ref, &arg_vregs, &result_vregs)`.
5. Post-call: for each out-param `(slot, N)`:
    - Load each scalar component from the slot:
      ```
      addr = slot_addr(slot)
      comp_0 = load(addr, 0)
      comp_1 = load(addr, 4)
      ...
      ```
    - Store into the local variable that the out-param references.
      The out-param in Naga is `Expression::LocalVariable(lv)`, so we
      need to map back to the VReg for that local.
    - For scalar out-params (1 component): just `Copy` to the local VReg.
    - For vector out-params: this is beyond scalar scope. For now, emit
      a `LowerError::UnsupportedExpression` if N > 1, or handle it with
      multiple loads into separate VRegs (if the caller accesses
      components individually).

   Actually, since this is scalar-only scope, LPFX functions that return
   vectors via out-pointers are not yet fully supported. We should:
    - Support scalar-returning LPFX (e.g. `lpfx_hash1`, noise functions
      that return scalar) fully.
    - For vector-returning LPFX (e.g. `lpfx_hsv2rgb`): emit
      `LowerError::UnsupportedExpression("LPFX vector out-param")` for now.

6. If `result` is `Some(expr_handle)`:
    - Cache the result VReg in `ctx.expr_cache[expr_handle]`.

### Statement::Call integration — `lower_stmt.rs`

In the `Statement::Call` match arm, replace the LPFX `todo!()`:

```rust
let callee_name = module.functions[*function].name.as_deref().unwrap_or("");
if callee_name.starts_with("lpfx_") {
    return lower_lpfx::lower_lpfx_call(ctx, *function, arguments, *result);
}
```

## Validate

```
cargo check -p lps-frontend
cargo +nightly fmt -p lps-frontend -- --check
```

After this phase, GLSL programs with scalar LPFX calls (hash, noise)
lower to complete LPIR with `@lpfx::...` imports.
