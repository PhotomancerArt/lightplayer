# Phase 3: Update Batch JIT Path

## Current flow

```rust
// compile_glsl_to_gl_module_jit:
let mut module = compiler.compile_to_gl_module_jit(source, target, max_errors)?;
match options.float_mode {
    DecimalFormat::Q32 => {
        let transform = Q32Transform::new(...);
        module = module.apply_transform(transform)?;
    }
    DecimalFormat::Float => {
        return Err("Float format not yet supported");
    }
}
```

`compile_to_gl_module_jit` internally compiles every function with
`FloatStrategy`, then `apply_transform` creates a new module and
rewrites all the IR.

## New flow

```rust
// compile_glsl_to_gl_module_jit:
let numeric_mode = match options.float_mode {
    DecimalFormat::Q32 => NumericMode::Q32(Q32Strategy::new(options.q32_opts)),
    DecimalFormat::Float => NumericMode::Float(FloatStrategy),
};
let module = compiler.compile_to_gl_module_jit(source, target, max_errors, numeric_mode)?;
// No transform step. Module is ready.
```

## Changes to compile_to_gl_module_jit

This function (in `glsl_compiler.rs`) compiles all functions and adds
them to the module. It needs to:

1. Accept `numeric_mode: NumericMode` parameter.
2. Pass `numeric_mode` to `compile_function_to_clif` /
   `compile_main_function_to_clif`.
3. Use `numeric_mode.scalar_type()` when building signatures for
   `GlModule::declare_function`.

The internal loop currently does:

```rust
for func in typed_ast.user_functions {
    let sig = SignatureBuilder::build_with_triple(...);
    let func_id = gl_module.declare_function(&func.name, Linkage::Local, &sig)?;
    // ... compile and define
}
```

With the change, `sig` is built with the correct scalar type from the
start. No `transform.transform_signature()` needed.

## Remove apply_transform call

Delete the `match options.float_mode` block in
`compile_glsl_to_gl_module_jit` that calls `apply_transform`. The module
produced by `compile_to_gl_module_jit` is already in Q32 form.

## Remove Float format rejection

The current code rejects `DecimalFormat::Float` with an error. With
the strategy pattern, Float is valid — it uses `FloatStrategy` and
produces float IR. Remove the rejection.

Note: Float mode still won't work end-to-end for JIT (TestCase
relocations aren't resolved), but that's a separate issue. The codegen
itself is correct.

## Validate

```bash
cargo check -p lp-glsl-compiler --features std
scripts/glsl-filetests.sh
```

This is the critical validation point — filetests must pass with Q32
direct emission.
