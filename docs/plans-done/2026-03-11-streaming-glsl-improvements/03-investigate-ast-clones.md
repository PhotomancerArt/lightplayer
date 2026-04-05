# Phase 3: Investigate T::clone_one AST Clones

## Problem

12,082 bytes (363 allocs) of `T::clone_one` appear at peak in the streaming
trace but not in the batch trace. Sub-entries:

- `Expr::clone` — 7,784 bytes, 139 allocs
- `Declaration::clone` — 1,848 bytes, 33 allocs
- `String::clone` — 770 bytes, 165 allocs
- `Box::clone` — 672 bytes, 8 allocs

These are `glsl` crate AST node clones. In the batch path, these clones happen
during CLIF generation but are freed before compilation peak. In the streaming
path, they're alive during `define_function`.

## Investigation

1. Check whether `source_loc_manager` or `source_map` accumulate data across
   loop iterations that includes cloned AST data. Both are passed as `&mut` to
   `compile_single_function_to_clif` and persist across the loop.

2. Check whether `FunctionSignature` insertion clones AST types:
   ```rust
   glsl_signatures.insert(
       func_info.name.clone(),
       FunctionSignature {
           name: func_info.name.clone(),
           return_type: func_info.typed_function.return_type.clone(),
           parameters: func_info.typed_function.parameters.clone(),
       },
   );
   ```
   `return_type` is `types::Type` and `parameters` is `Vec<Parameter>` — these
   are semantic types, not AST nodes. Probably not the cause.

3. Check whether any per-function allocations from CLIF generation are surviving
   because they're referenced by the `GlslCompiler` instance (which persists
   across the loop via `let mut compiler = GlslCompiler::new();` outside the loop).
   The `FunctionBuilderContext` inside `GlslCompiler` might retain allocations
   between uses.

4. If the compiler retains state, try creating a fresh `GlslCompiler::new()`
   inside each loop iteration to force cleanup.

## Fix

Depends on investigation findings. Likely one of:

- Move `GlslCompiler::new()` inside the loop
- Clear `source_loc_manager` / `source_map` if they're accumulating
- Drop the `FunctionBuilderContext` between iterations

## Expected savings

~12 KB if the clones can be eliminated or freed before the compilation peak.

## Validate

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std -- test_streaming
```

Re-run on ESP32 emulator with heap tracing to verify the `T::clone_one` entries
are gone or reduced at peak.
