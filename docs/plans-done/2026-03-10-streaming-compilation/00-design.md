# Streaming Per-Function Compilation — Design

## Scope

Refactor the GLSL → machine code JIT pipeline to compile functions one at a time
(streaming), freeing each function's AST, CLIF IR, and codegen working set before
starting the next function. Goal: reduce peak heap usage on ESP32 by ~25-30 KB.

- JIT path only (not emulator/ObjectModule)
- New `glsl_jit_streaming` entry point alongside existing `glsl_jit`
- Q32 transform uses two-module approach (not in-place)
- Functions compiled smallest-first (by recursive AST node count)
- Unused GlModule metadata never stored in Q32 module

## File Structure

```
lp-shader/lp-glsl-compiler/src/
├── frontend/
│   ├── mod.rs                          # UPDATE: Add glsl_jit_streaming() entry point
│   ├── glsl_compiler.rs                # UPDATE: Extract per-function CLIF gen method
│   └── semantic/
│       └── mod.rs                      # UPDATE: Add ast_node_count() to TypedFunction
├── backend/
│   ├── codegen/
│   │   └── jit.rs                      # UPDATE: Add build_jit_executable_streaming()
│   ├── module/
│   │   └── gl_module.rs                # UPDATE: Add per-function transform helper
│   └── transform/
│       └── pipeline.rs                 # No changes (Transform trait already per-function)
└── exec/
    └── jit.rs                          # No changes (GlslJitModule unchanged)
```

## Architecture

```
                          TypedShader (AST)
                               │
                    ┌──────────┴──────────┐
                    │  Sort functions by   │
                    │  AST node count      │
                    └──────────┬──────────┘
                               │
            ┌──────────────────┼──────────────────┐
            │                  │                   │
     Float Module         Q32 Module          Output Maps
     (declarations         (declarations      (built incrementally)
      only, ~17KB)          + compilation)
            │                  │                   │
            │    ┌─────────────┴────────┐          │
            │    │  For each function:  │          │
            │    │                      │          │
            ├───►│  1. CLIF gen         │          │
            │    │     (uses float mod) │          │
            │    │                      │          │
            │    │  2. Q32 transform    │          │
            │    │     (float→Q32 func) │          │
            │    │                      │          │
            │    │  3. define_function  │          │
            │    │     (compile→mcode)  │          │
            │    │                      │          │
            │    │  4. Free CLIF IR     ├─────────►│ collect sig + name
            │    │                      │          │
            │    └─────────────┬────────┘          │
            │                  │                   │
            │    ┌─────────────┴────────┐          │
            │    │ finalize_definitions │          │
            │    └─────────────┬────────┘          │
            │                  │                   │
    drop ◄──┘    ┌─────────────┴────────┐          │
                 │ get_finalized_fn ptrs├─────────►│
                 └─────────────┬────────┘          │
                               │                   │
                         ┌─────┴─────┐      ┌──────┴──────┐
                         │ JITModule │      │  sig maps   │
                         └─────┬─────┘      └──────┬──────┘
                               │                   │
                         ┌─────┴───────────────────┴─────┐
                         │         GlslJitModule         │
                         └───────────────────────────────┘
```

## Main Components

### 1. `glsl_jit_streaming()` (frontend/mod.rs)

New top-level entry point. Orchestrates the entire streaming pipeline:

- Parses and analyzes GLSL → `TypedShader`
- Creates float module + Q32 module (both with all functions declared upfront)
- Builds func_id_map / old_func_id_map for the transform
- Sorts functions by AST node count (ascending)
- Runs the per-function loop
- Calls `finalize_definitions`, extracts pointers, returns `GlslJitModule`

### 2. Per-function CLIF generation (frontend/glsl_compiler.rs)

Extracts the existing `compile_function_to_clif` logic so it can be called for
a single function without needing all functions' IR to be stored first. The
method signature stays the same — it already takes a single `TypedFunction` and
returns a `Function`. The key change is that it's called from the streaming loop
rather than a batch loop that collects all results into a Vec.

### 3. Per-function transform (backend/module/gl_module.rs)

New helper that transforms a single function using the existing `Transform` trait.
Takes:

- A float-typed `Function` (from CLIF gen)
- The Q32 transform
- A `TransformContext` referencing the Q32 module
- The func_id_map / old_func_id_map

Returns the transformed `Function`. Does NOT store it in the Q32 module's `fns` —
instead it goes straight to `define_function`.

### 4. `build_jit_executable_streaming()` (backend/codegen/jit.rs)

Inner loop that, for each function:

1. Generates CLIF IR (via the float module)
2. Transforms (Q32)
3. Calls `define_function` on the Q32 module
4. Clears the context (frees CLIF IR + codegen working set)
5. Collects the function's signature metadata for the output

After the loop: `finalize_definitions`, extract pointers, build `GlslJitModule`.

### 5. AST node counting (frontend/semantic/mod.rs)

Simple recursive count of AST nodes in a `TypedFunction`. Used to sort functions
smallest-first before the compilation loop.

## Key Decisions

- **Two modules**: Float module for CLIF gen FuncRef resolution, Q32 module for
  compilation. Float module is declarations-only (~17 KB).
- **Q32 transform NOT in-place**: Preserved. Transform takes float `Function`,
  returns Q32 `Function`. Two-module approach maintained.
- **Declaration order**: Alphabetical (matching current transform behavior) for
  deterministic FuncId assignment. Main gets `Linkage::Export`.
- **Compilation order**: Ascending AST node count. Separate from declaration order.
- **No unused metadata in Q32 module**: `source_text`, `source_loc_manager`,
  `source_map`, `function_registry` never stored. `glsl_signatures` and
  `cranelift_signatures` built incrementally during the loop.
- **Existing paths preserved**: `glsl_jit` with `memory_optimized: true/false`
  remains untouched. New `glsl_jit_streaming` added alongside.
