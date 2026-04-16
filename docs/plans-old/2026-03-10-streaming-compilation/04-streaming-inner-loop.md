# Phase 4: Streaming Inner Loop

## Scope

Replace the temporary batch implementation in `glsl_jit_streaming` (from Phase 2)
with the actual per-function streaming loop. This is the core of the plan.

For each function (in smallest-first order):

1. Generate CLIF IR from AST (using float module)
2. Q32 transform (using per-function helper from Phase 3)
3. `define_function` on Q32 module (compile to machine code)
4. Free CLIF IR and codegen context
5. Collect signature metadata for output

After the loop: `finalize_definitions`, extract function pointers, build
`GlslJitModule`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### 1. Refactor `glsl_jit_streaming` inner loop

Replace the TODO/temporary batch path from Phase 2 with the streaming loop.

The main function in `frontend/mod.rs` should look roughly like this
(pseudocode — adapt to actual types and borrow checker):

```rust
pub fn glsl_jit_streaming(
    source: &str,
    options: GlslOptions,
) -> Result<Box<dyn GlslExecutable>, GlslDiagnostics> {
    // --- Setup (from Phase 2) ---
    // 1. Validate, build target, parse, create ISA
    // 2. Create float_module and q32_module
    // 3. Declare all functions in both modules
    // 4. Build func_id_map, old_func_id_map (include builtins)
    // 5. Sort functions by ast_node_count (ascending)

    // --- Streaming loop ---
    let q32_transform = Q32Transform::new(FixedPointFormat::Fixed16x16)
        .with_q32_opts(options.q32_opts);

    let mut glsl_signatures = HashMap::new();
    let mut cranelift_signatures = HashMap::new();

    // We need source_loc_manager and source_map for CLIF generation
    let mut source_loc_manager = SourceLocManager::new();
    let mut source_map = GlSourceMap::new();
    let main_file_id = source_map.add_file(
        GlFileSource::Synthetic(String::from("main.glsl")),
        String::from(source),
    );

    // Compile functions smallest-first
    for func_info in &sorted_functions {
        let typed_func: &TypedFunction = func_info.typed_function;
        let float_func_id = func_info.float_func_id;
        let q32_func_id = func_info.q32_func_id;
        let linkage = func_info.linkage;

        // Step 1: Generate CLIF IR (uses float_module for FuncRef resolution)
        let mut compiler = GlslCompiler::new();
        let float_clif = compiler.compile_function_to_clif(
            typed_func,
            float_func_id,
            &float_func_ids,           // all function IDs in float module
            &typed_ast.function_registry,
            &typed_ast.global_constants,
            &mut float_module,
            isa_ref.as_ref(),
            &mut source_loc_manager,
            &mut source_map,
            main_file_id,
        )?;

        // Step 2: Q32 transform (float Function → Q32 Function)
        let q32_clif = transform_single_function(
            &float_clif,
            &q32_transform,
            &mut q32_module,
            &func_id_map,
            &old_func_id_map,
            q32_func_id,
        )?;
        drop(float_clif); // Free float CLIF IR

        // Step 3: Compile to machine code
        let mut ctx = q32_module.module_internal().make_context();
        ctx.func = q32_clif;
        q32_module
            .module_mut_internal()
            .define_function(q32_func_id, &mut ctx)
            .map_err(/* error handling */)?;
        q32_module.module_internal().clear_context(&mut ctx);
        // ctx and q32_clif are now freed

        // Step 4: Collect metadata
        let q32_sig = q32_transform.transform_signature(&float_sig);
        cranelift_signatures.insert(func_info.name.clone(), q32_sig);
        glsl_signatures.insert(func_info.name.clone(), FunctionSignature {
            name: func_info.name.clone(),
            return_type: typed_func.return_type.clone(),
            parameters: typed_func.parameters.clone(),
        });
    }

    // --- Finalize ---
    // Drop float module (no longer needed)
    drop(float_module);

    q32_module
        .module_mut_internal()
        .finalize_definitions()?;

    // Extract function pointers
    let mut function_ptrs = HashMap::new();
    for func_info in &sorted_functions {
        let ptr = q32_module
            .module_internal()
            .get_finalized_function(func_info.q32_func_id);
        function_ptrs.insert(func_info.name.clone(), ptr);
    }

    // Extract target info
    let call_conv = q32_module.target.default_call_conv()?;
    let pointer_type = q32_module.target.pointer_type()?;

    // Extract JITModule, dropping rest of q32 GlModule
    let jit_module = q32_module.into_module();

    Ok(Box::new(GlslJitModule {
        jit_module,
        function_ptrs,
        signatures: glsl_signatures,
        cranelift_signatures,
        call_conv,
        pointer_type,
    }))
}
```

### 2. Key implementation details

**Borrow checker with `compile_function_to_clif`**: The existing method takes
`&mut GlModule<M>` (the float module) and borrows the `TypedFunction` from
`typed_ast`. Since `typed_ast` and `float_module` are separate locals, this
should work. But if there are borrow conflicts (e.g., `GlslCompiler::new()` in
the loop), either create the compiler once outside the loop or create a fresh
one each iteration (the `FunctionBuilderContext` inside is lightweight).

**`compile_function_to_clif` visibility**: Currently this is a private method on
`GlslCompiler`. For the streaming path, it needs to be callable from
`frontend/mod.rs`. It's already `fn compile_function_to_clif` (not `pub`).
Either:

- Make it `pub(crate)` so `frontend/mod.rs` can call it
- Or call through `GlslCompiler` by adding a thin public wrapper

The method already takes all its dependencies as parameters (func_ids, registry,
constants, module, isa, etc.), so it should work unchanged.

**`compile_main_function_to_clif` vs `compile_function_to_clif`**: Main uses
`compile_main_function_to_clif` which passes `source_text` for error messages.
In the streaming path, both user functions and main go through the same loop.
Use `compile_function_to_clif_impl` directly, passing `Some(source)` for main
and `None` for user functions (matching current behavior).

**Creating `GlslJitModule` directly**: The streaming function builds
`GlslJitModule` directly rather than going through `build_jit_executable`.
Import `GlslJitModule` struct and its fields. Since `GlslJitModule` is in
`exec/jit.rs` with `pub(crate)` fields, this should work from within the crate.

**Error handling for `define_function`**: Copy the verifier error handling from
`build_jit_executable_memory_optimized` (the block that checks for verifier
errors and re-runs verification for better messages).

### 3. Function info struct

Create a small struct to hold per-function metadata during the loop:

```rust
struct StreamingFuncInfo<'a> {
    name: String,
    typed_function: &'a TypedFunction,
    float_func_id: FuncId,
    q32_func_id: FuncId,
    float_sig: Signature,
    linkage: Linkage,
    ast_size: usize,
}
```

Build a `Vec<StreamingFuncInfo>` during setup, sort by `ast_size`, then iterate.

### 4. Tests

Update the tests from Phase 2 to verify actual correctness (calling the
compiled function and checking return values):

```rust
#[test]
#[cfg(feature = "std")]
fn test_streaming_returns_correct_value() {
    let source = r#"
        vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
            return vec4(1.0, 0.0, 0.0, 1.0);
        }
    "#;
    let options = GlslOptions::q32_jit();
    let mut streaming = glsl_jit_streaming(source, options.clone()).unwrap();
    let mut batch = glsl_jit(source, options).unwrap();

    // Both should produce the same result
    let streaming_result = streaming.call_i32("main", &[]).unwrap();
    let batch_result = batch.call_i32("main", &[]).unwrap();
    assert_eq!(streaming_result, batch_result);
}
```

Add a test with the full rainbow shader (or a representative multi-function
shader) to verify cross-function calls work:

```rust
#[test]
#[cfg(feature = "std")]
fn test_streaming_multi_function_cross_calls() {
    let source = r#"
        float double_it(float x) {
            return x * 2.0;
        }
        float quad_it(float x) {
            return double_it(double_it(x));
        }
        vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
            float v = quad_it(0.25);
            return vec4(v, 0.0, 0.0, 1.0);
        }
    "#;
    let options = GlslOptions::q32_jit();
    let mut streaming = glsl_jit_streaming(source, options.clone()).unwrap();
    let mut batch = glsl_jit(source, options).unwrap();

    let streaming_result = streaming.call_i32("main", &[]).unwrap();
    let batch_result = batch.call_i32("main", &[]).unwrap();
    assert_eq!(streaming_result, batch_result);
}
```

### 5. Remove temporary batch fallback

Remove the temporary batch implementation added in Phase 2. The function should
now use only the streaming loop.

## Validate

```bash
cd lp-shader/lps-compiler && cargo test --features std -- test_streaming
cd lp-shader/lps-compiler && cargo test --features std
```
