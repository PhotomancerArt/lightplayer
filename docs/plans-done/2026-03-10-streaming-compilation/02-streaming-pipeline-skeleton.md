# Phase 2: Streaming Pipeline Skeleton

## Scope

Create the `glsl_jit_streaming` entry point and the two-module setup. This phase
gets the skeleton compiling but does NOT implement the per-function loop yet —
that's Phase 4. This phase focuses on:

- The new public API function
- Creating both float and Q32 modules with all declarations
- Building the func_id_map / old_func_id_map for transforms
- Sorting functions by AST node count
- Exporting the new function from the crate

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### 1. Add `glsl_jit_streaming` to `frontend/mod.rs`

Add a new public function alongside `glsl_jit`. It takes the same arguments
(`source: &str`, `options: GlslOptions`) and returns the same type
(`Result<Box<dyn GlslExecutable>, GlslDiagnostics>`).

The function should:

1. **Validate options** (same as `compile_glsl_to_gl_module_jit`)

2. **Build target** (same as `compile_glsl_to_gl_module_jit` — reuse the
   existing target creation logic)

3. **Parse and analyze** — call `CompilationPipeline::parse_and_analyze`

4. **Create ISA** for signature building (same as `compile_to_gl_module_jit`)

5. **Create float module** — `GlModule::new_jit(target.clone())`. This module
   holds float-type declarations and is used for CLIF IR generation.

6. **Create Q32 module** — `GlModule::new_jit(target)`. This module holds
   Q32-type declarations and is used for compilation.

7. **Declare all functions in both modules**:
    - Collect all function names + signatures from `typed_ast.user_functions`
      and `typed_ast.main_function`
    - Sort alphabetically for deterministic FuncId assignment
    - For each function:
        - Build float signature via `SignatureBuilder::build_with_triple`
        - Build Q32 signature via `q32_transform.transform_signature(&float_sig)`
        - Declare in float module with float sig
        - Declare in Q32 module with Q32 sig
        - Main gets `Linkage::Export`, user functions get `Linkage::Local`

8. **Build func_id_map and old_func_id_map** — same logic as
   `apply_transform_impl` in `gl_module.rs`. Map function names to their FuncIds
   in both modules, and build the old→new FuncId mapping. Include builtins.

9. **Sort functions by AST node count** (ascending) — use
   `TypedFunction::ast_node_count()` from Phase 1.

10. **TODO: Per-function loop** — for now, just add a TODO comment where the
    streaming loop will go in Phase 4. To make the function compile, you can
    call the existing batch path as a temporary implementation:
    - Generate all CLIF IR (batch, like current `compile_to_gl_module_jit`)
    - Apply Q32 transform (batch)
    - Call `build_jit_executable_memory_optimized`

    This lets us validate the setup (parsing, module creation, declarations)
    without the streaming loop yet.

### 2. Export from crate

File: `lp-shader/lp-glsl-compiler/src/lib.rs`

Add `pub use frontend::glsl_jit_streaming;` alongside the existing
`pub use frontend::glsl_jit;`.

### 3. Test

Add a basic test that calls `glsl_jit_streaming` with a simple shader and
verifies it produces a working executable. This can go in
`lp-shader/lp-glsl-compiler/src/frontend/mod.rs` or as an integration test.

```rust
#[test]
#[cfg(feature = "std")]
fn test_glsl_jit_streaming_basic() {
    let source = r#"
        vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
            return vec4(1.0, 0.0, 0.0, 1.0);
        }
    "#;
    let options = GlslOptions::q32_jit();
    let mut executable = glsl_jit_streaming(source, options).unwrap();
    // Just verify it compiled successfully and has a main function
    assert!(executable.get_direct_call_info("main").is_some());
}
```

Also test with a multi-function shader to verify cross-function declarations
work:

```rust
#[test]
#[cfg(feature = "std")]
fn test_glsl_jit_streaming_multi_function() {
    let source = r#"
        float helper(float x) {
            return x * 2.0;
        }
        vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
            float v = helper(0.5);
            return vec4(v, 0.0, 0.0, 1.0);
        }
    "#;
    let options = GlslOptions::q32_jit();
    let mut executable = glsl_jit_streaming(source, options).unwrap();
    assert!(executable.get_direct_call_info("main").is_some());
}
```

## Validate

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std -- test_glsl_jit_streaming
```

Ensure all existing tests still pass:

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std
```
