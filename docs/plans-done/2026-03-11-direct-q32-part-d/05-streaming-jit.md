# Phase 5: Simplify Streaming JIT Path

This is the biggest change in Plan D. The current streaming path is
~200 lines of complex plumbing to manage two modules and per-function
transforms. With direct emission, it becomes a straightforward
single-module loop.

## Current flow

```
1. Parse → TypedShader
2. Create float_module and q32_module
3. For each function:
   a. Build float sig → declare in float_module
   b. Transform sig → declare in q32_module
   c. Build func_id_map / old_func_id_map
4. Declare builtins in both modules, populate maps
5. For each function (sorted by AST size):
   a. compile_single_function_to_clif → float CLIF (in float_module)
   b. transform_single_function → q32 CLIF (in q32_module)
   c. define_function in q32_module
   d. drop float CLIF
6. drop float_module
7. finalize q32_module → JIT executable
```

## New flow

```
1. Parse → TypedShader
2. Create single module
3. Construct NumericMode::Q32(Q32Strategy)
4. For each function:
   a. Build sig with scalar_float_type=I32 → declare in module
5. For each function (sorted by AST size):
   a. compile_single_function_to_clif with Q32Strategy → Q32 CLIF
   b. define_function in module
   c. drop CLIF
6. finalize module → JIT executable
```

## What gets removed

- `float_module` — entirely gone
- `q32_module` naming — just `module`
- `func_id_map: HashMap<String, FuncId>` — only needed by transform
- `old_func_id_map: HashMap<FuncId, String>` — only needed by transform
- `float_func_ids: HashMap<String, FuncId>` — only for float module
- `StreamingFuncInfo.float_func_id` — no float module
- `transform_single_function` call — no transform
- `Q32Transform` construction — no transform
- Builtin declaration in float_module — no float module
- The transform import and all related `use` statements

## What stays

- Parse + semantic analysis
- Function sorting by AST size (memory optimization)
- Single module creation + builtin declaration
- Per-function compilation + define_function + clear
- Signature collection for JIT executable metadata
- `build_jit_executable_streaming` call

## StreamingFuncInfo simplification

```rust
struct StreamingFuncInfo<'a> {
    name: String,
    typed_function: &'a TypedFunction,
    func_id: FuncId,           // was q32_func_id
    linkage: Linkage,
    ast_size: usize,
}
```

## Function ID map simplification

The streaming path currently maintains `jit_func_id_map` for the
executable builder. This stays — it maps function names to their
FuncIds in the single module. But it's just the declarations from
step 4, no float/Q32 indirection.

## Implementation approach

Rather than editing the existing 200-line function in place, it may
be cleaner to rewrite `glsl_jit_streaming` from scratch and delete
the old version. The new version is ~80-100 lines.

## Validate

```bash
cargo check -p lps-compiler --features std
cargo test -p lps-compiler --features std -- streaming
scripts/glsl-filetests.sh
```

The existing streaming tests (`test_glsl_jit_streaming_basic`,
`test_streaming_returns_correct_value`, etc.) validate correctness.
