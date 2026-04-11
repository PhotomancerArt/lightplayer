# Phase 2: Add NumericMode Parameter to Codegen Functions

## Current state

`compile_function_to_clif_impl` hardcodes `NumericMode::Float(FloatStrategy)`:

```rust
let mut codegen_ctx = CodegenContext::new(
    builder,
    gl_module,
    source_map,
    file_id,
    NumericMode::Float(FloatStrategy),
);
```

## Changes

Add `numeric_mode: NumericMode` parameter to:

1. `compile_function_to_clif_impl` — the core function
2. `compile_function_to_clif` — wrapper
3. `compile_main_function_to_clif` — wrapper
4. `compile_single_function_to_clif` — public, used by streaming path

Each wrapper passes the parameter through to `compile_function_to_clif_impl`.

In `compile_function_to_clif_impl`, use the parameter:

```rust
let mut codegen_ctx = CodegenContext::new(
    builder,
    gl_module,
    source_map,
    file_id,
    numeric_mode,
);
```

Also use `numeric_mode.scalar_type()` when building the signature:

```rust
let sig = SignatureBuilder::build_with_triple(
    &func.return_type,
    &func.parameters,
    pointer_type,
    triple,
    numeric_mode.scalar_type(),  // NEW
);
```

## Callers to update

- `compile_to_gl_module_jit` (batch compilation) — calls
  `compile_function_to_clif` and `compile_main_function_to_clif`
  for each function. These need to pass the mode.
- `glsl_jit_streaming` — calls `compile_single_function_to_clif`.
  Needs to pass the mode.
- `compile_to_gl_module_object` (emulator) — same as batch.

For now, all callers pass `NumericMode::Float(FloatStrategy)` —
no behavioral change yet. Phases 3-5 change the callers to pass
Q32Strategy when appropriate.

## NumericMode construction

The pipeline knows `GlslOptions` which has `float_mode` and
`q32_opts`. Constructing the mode:

```rust
let numeric_mode = match options.float_mode {
    DecimalFormat::Q32 => NumericMode::Q32(Q32Strategy::new(options.q32_opts)),
    DecimalFormat::Float => NumericMode::Float(FloatStrategy),
};
```

This construction happens at the pipeline entry points (Phases 3-5).

## Validate

```bash
cargo check -p lps-compiler --features std
scripts/glsl-filetests.sh
```
