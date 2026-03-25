# Phase 2: Naga — param qualifiers, metadata output, function-scoped errors

## Scope

- Extend **`FunctionInfo`** / **`function_info()`** with per-parameter
  **`GlslParamQualifier`** (derive from Naga `FunctionArgument`: pointer in
  `Function` address space → `InOut`/`Out` per existing lowering conventions).
- Change **`lower(naga_module) -> Result<(IrModule, GlslModuleMeta), LowerError>`**.
  Build **`GlslModuleMeta`** in lockstep with `mb.add_function(ir)` — same order
  as `IrModule::functions`.
- Add **`LowerError::InFunction { name: String, inner: Box<LowerError> }`** (or
  equivalent) and wrap **`lower_function`** results in **`lower.rs`** so every
  failure carries the GLSL function name.
- Replace uses of **`GlslType`** with **`lpir::GlslType`** if Phase 1 moved it.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Qualifiers from Naga

Inspect each `function.arguments[i]`: if `ty` is `TypeInner::Pointer` with
`Function` address space, map to `Out` vs `InOut` (match how `lower_ctx` treats
`pointer_args` — read-only vs mutable). Value types → `In`.

### Metadata construction

When emitting each `IrFunction` in the loop in `lower.rs`, push the corresponding
`GlslFunctionMeta` (name + params with qualifiers + return type).

### Display for `InFunction`

Implement `Display` so users see: `in function 'myShader': unsupported expression: …`

### Tests

- Update tests that call **`lower()`** — now unwrap tuple `(_, meta)`.
- Add one test where lowering fails inside a named function and assert error
  **contains** the function name.

## Validate

```
cargo check -p lp-glsl-naga
cargo test -p lp-glsl-naga
```
