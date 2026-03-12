# Phase 4: Update Object/Emulator Path

## Current flow

```rust
// compile_glsl_to_gl_module_object:
let mut module = compiler.compile_to_gl_module_object(source, target, max_errors)?;
let original_clif = format_clif_module(&module).ok();
match options.decimal_format {
    DecimalFormat::Q32 => {
        module = module.apply_transform(transform)?;
        let transformed_clif = format_clif_module(&module).ok();
    }
    DecimalFormat::Float => { /* no transform */ }
}
Ok((module, original_clif, transformed_clif))
```

## New flow

```rust
let numeric_mode = match options.decimal_format {
    DecimalFormat::Q32 => NumericMode::Q32(Q32Strategy::new(options.q32_opts)),
    DecimalFormat::Float => NumericMode::Float(FloatStrategy),
};
let module = compiler.compile_to_gl_module_object(source, target, max_errors, numeric_mode)?;
let clif = format_clif_module(&module).ok();
// No transform. original and transformed are the same.
Ok((module, clif.clone(), clif))
```

## CLIF capture

The function returns `(module, original_clif, transformed_clif)`. With
no transform, these are identical. We could simplify the return type to
a single CLIF string, but that changes the API for `build_executable`
and other callers.

**Approach**: Keep the return type. Pass the same CLIF for both. This
is a minimal change. Cleanup of the return type can happen in Plan E.

## Changes to compile_to_gl_module_object

Same pattern as `compile_to_gl_module_jit` in Phase 3: accept
`numeric_mode`, pass to internal compilation functions, no transform.

## Validate

```bash
cargo check -p lp-glsl-compiler --features std,emulator
cargo test -p lp-glsl-compiler --features std,emulator
```
