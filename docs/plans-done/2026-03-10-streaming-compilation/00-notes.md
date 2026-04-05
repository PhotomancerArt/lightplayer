# Streaming Per-Function Compilation

## Scope of Work

Refactor the GLSL → machine code pipeline to compile functions one at a time
(streaming), freeing each function's AST, CLIF IR, and codegen working set
before starting the next function. Goal: reduce peak heap usage on ESP32.

Currently, the pipeline works in three batch phases:

1. **CLIF IR generation** — all functions' CLIF IR built and stored in `GlModule.fns`
2. **Q32 transform** — creates a *second* `GlModule<JITModule>`, transforms all
   functions, drops old module
3. **Machine code compilation** — `build_jit_executable_memory_optimized` compiles
   one-by-one, freeing CLIF IR after each

The proposed pipeline:

1. Parse + analyze → `TypedShader` (AST for all functions)
2. Declare all functions in both modules (float + Q32 signatures) upfront
3. For each function (smallest first):
   a. Generate CLIF IR from AST (using float module for FuncRef resolution)
   b. Q32 transform (using Q32 module — NOT in-place)
   c. `define_function` → compile to machine code
   d. Free AST, float CLIF IR, Q32 CLIF IR
4. `finalize_definitions` → patch relocations
5. Extract function pointers

## Current State

### Pipeline code locations

| File                                                           | Role                                                                   |
|----------------------------------------------------------------|------------------------------------------------------------------------|
| `lp-shader/lp-glsl-compiler/src/frontend/mod.rs`               | `glsl_jit`, `compile_glsl_to_gl_module_jit` — top-level API            |
| `lp-shader/lp-glsl-compiler/src/frontend/glsl_compiler.rs`     | `GlslCompiler::compile_to_gl_module_jit` — CLIF IR generation loop     |
| `lp-shader/lp-glsl-compiler/src/backend/module/gl_module.rs`   | `GlModule`, `apply_transform`, `add_function`, `declare_function`      |
| `lp-shader/lp-glsl-compiler/src/backend/codegen/jit.rs`        | `build_jit_executable`, `build_jit_executable_memory_optimized`        |
| `lp-shader/lp-glsl-compiler/src/backend/transform/pipeline.rs` | `Transform` trait, `TransformContext`                                  |
| `lp-shader/lp-glsl-compiler/src/backend/transform/q32/`        | Q32 transform implementation                                           |
| `lp-shader/lp-glsl-compiler/src/frontend/codegen/context.rs`   | `CodegenContext` — needs `&mut GlModule<M>` for `declare_func_in_func` |
| `lp-shader/lp-glsl-compiler/src/exec/jit.rs`                   | `GlslJitModule` — final output                                         |

### Key data structures

- `TypedShader` — parsed GLSL AST. Contains `Vec<TypedFunction>` where each has
  a `body: Vec<glsl::syntax::Statement>` (the AST nodes).
- `GlModule<M>` — owns cranelift `Module`, `fns: HashMap<String, GlFunc>`,
  plus metadata (function_registry, source_text, source_loc_manager, source_map,
  glsl_signatures).
- `GlFunc` — per-function: `name`, `clif_sig`, `func_id`, `function: Function` (CLIF IR).
- `GlslJitModule` — final result: owns `JITModule`, function pointers, signatures.

### Current memory at peak (228 KB of 320 KB heap)

Peak occurs during `fastalloc::run` (regalloc) inside `define_function`. At peak:

- CLIF IR for remaining uncompiled functions: ~25-30 KB
- Codegen working set (one function): ~70 KB
- Module declarations/metadata: ~17 KB
- Frontend metadata in GlModule (dead weight): ~10-15 KB
- Collections (ChunkedVec/Map): ~28 KB
- Non-compilation baseline (server, fs, lpfx): ~17 KB
- Compiled machine code: ~6 KB

### Test shader

The `examples/basic` rainbow shader has 11 functions (10 user + main), ranging
from 3-line palette functions to a 27-line main function with complex expressions.

### Existing memory_optimized path

`build_jit_executable_memory_optimized` already compiles functions one-by-one and
frees CLIF IR after each. But ALL CLIF IR is already generated and transformed
before this loop starts.

### Q32 transform constraint

The Q32 transform MUST NOT be done in-place. The current two-module approach
(old float module → new Q32 module) was adopted after a failed attempt at
in-place transformation. This constraint must be preserved.

The transform uses `TransformContext` which needs:

- `&mut GlModule<M>` (the new/target module)
- `func_id_map: HashMap<String, FuncId>` (name → new FuncId)
- `old_func_id_map: HashMap<FuncId, String>` (old FuncId → name)

The `Transform::transform_function` takes a single `&Function` and returns a
new `Function`. It already works per-function conceptually.

## Questions

### Q1: Two JITModules or float module is lightweight?

The streaming approach needs CLIF IR generation to use float-type signatures
(for `declare_func_in_func` when emitting cross-function calls), but the final
module needs Q32 signatures for `define_function`.

**Current approach**: One JITModule with float sigs → Q32 transform creates
second JITModule with Q32 sigs.

**Options**:

- (A) Keep two `GlModule<JITModule>`s — a "float module" for CLIF generation and
  a "Q32 module" for compilation. The float module never compiles, just holds
  declarations. After all functions are processed, drop the float module.
- (B) Use a single module with Q32 signatures, and do CLIF generation against a
  lightweight signature table (not a full module). Would require refactoring
  `CodegenContext` to not need `&mut GlModule<M>`.

**Suggestion**: Option A. The float module is lightweight (just declarations,
~17 KB for JITModule metadata). It's architecturally clean and keeps
`CodegenContext` unchanged. The memory cost of having two modules' declarations
alive simultaneously is ~34 KB total, but only during compilation (the float
module can be dropped once all CLIF IR is generated for each function).

Actually — with streaming, the float module can be kept alive the entire time
since it never holds CLIF IR (that's generated and consumed per-function). It's
just declarations. So the cost is a constant ~17 KB overhead.

**Answer**: Option A — two `GlModule<JITModule>`s. The float module is
lightweight (~17 KB declarations only) and avoids touching `CodegenContext`.
Future work: consider eliminating the float module entirely by refactoring
`CodegenContext` to use a lightweight signature table instead of `&mut GlModule<M>`
(see `future-work.md`).

### Q2: Function compilation order — how to determine "smallest"?

The plan calls for compiling smallest functions first to free memory for larger
ones. But "smallest" could mean:

- Fewest AST statements (known at parse time)
- Fewest CLIF IR instructions (known only after CLIF generation)
- Estimated from parameter/return type complexity

**Suggestion**: Use AST statement count as the heuristic. It's available after
parsing and correlates well with CLIF IR size. Count `TypedFunction.body.len()`
(or recursively count nested statements). This avoids needing to generate IR
just to determine size.

**Answer**: Use recursive AST node count as the heuristic. Simple, available
after parsing, good enough. Future improvement: a trait for estimating size
more accurately (see `future-work.md`).

### Q3: What to do about GlModule metadata during compilation?

`GlModule` carries `function_registry`, `source_text`, `source_loc_manager`,
`source_map`, and `glsl_signatures` through the entire pipeline. None of these
are used during `define_function` (machine code compilation). They're dead weight
at peak.

**Options**:

- (A) Drop them early in the streaming pipeline (before compilation loop)
- (B) Don't store them in GlModule at all in the streaming path — extract what's
  needed for `GlslJitModule` upfront and never put the rest in GlModule

**Suggestion**: Option B for the streaming path. The streaming function can
extract `glsl_signatures` and `cranelift_signatures` during the per-function
loop, and never bother putting `source_text`, `source_loc_manager`, `source_map`,
or `function_registry` into the Q32 module.

**Answer**: Option B — don't store unused metadata in the Q32 module. Build
`glsl_signatures` and `cranelift_signatures` incrementally during the per-function
loop. `source_text`, `source_loc_manager`, `source_map`, `function_registry`
never enter the Q32 module.

### Q4: Should the existing non-streaming path be preserved?

The current `build_jit_executable` and `build_jit_executable_memory_optimized`
are used by tests (with `memory_optimized: false/true`). The streaming approach
is a third path or a replacement for `memory_optimized`.

**Options**:

- (A) Add a new `build_jit_executable_streaming` alongside existing functions
- (B) Replace `build_jit_executable_memory_optimized` with the streaming approach
- (C) Replace both with streaming

**Suggestion**: Option A initially. Keep existing paths for tests and non-embedded
use. Add the streaming path as a new function gated on a flag or called directly
from the embedded JIT path. Once validated, consider deprecating the old
`memory_optimized` path.

**Answer**: Option A — add new `glsl_jit_streaming` alongside existing paths.
Existing `glsl_jit` with `memory_optimized: true/false` stays untouched. ESP32
callsite switches to the new function. Come back later to consolidate
(see `future-work.md`).

### Q5: Scope — JIT-only or also ObjectModule (emulator)?

The streaming approach could also apply to the `compile_to_gl_module_object` +
`build_emu_executable` path. Should this plan cover both?

**Suggestion**: JIT-only for this plan. The emulator path is used for development/
testing and isn't memory-constrained. The JIT path is the one running on ESP32.

**Answer**: JIT-only. Emulator/ObjectModule path is out of scope.

### Q6: How to handle `main` function linkage?

Currently `main` is declared with `Linkage::Export` while user functions use
`Linkage::Local`. In the streaming pipeline, all functions need to be declared
upfront. The order of declaration matters for FuncId assignment.

**Suggestion**: Declare all user functions first (sorted alphabetically for
deterministic FuncIds), then declare main with `Linkage::Export`. This matches
the current behavior. The compilation order (smallest-first) is separate from
declaration order.

**Answer**: Declare all functions upfront sorted alphabetically. Main gets
`Linkage::Export`, user functions get `Linkage::Local`. Compilation order
(smallest-first) is separate from declaration order.

## Notes

- The Q32 transform was previously attempted in-place and it failed badly. The
  current two-module approach is non-negotiable.
- `apply_transform_impl` currently iterates sorted by name and clones
  `func_id_map` for each function. With streaming, this becomes per-function
  and the maps are built once.
- `CodegenContext` takes `&mut GlModule<M>` mainly for `get_builtin_func_ref`
  (calls `module.declare_func_in_func`). This is the main dependency on having
  a live module during CLIF generation.

## Follow-up: Streaming GLSL Improvements (2026-03-11)

Phase 1: Bypass `GlModule::declare_function` in streaming — use
`module_mut_internal().declare_function()` to skip creating placeholder GlFunc
entries (~22 KB savings).

Phase 2: Borrow `func_id_map` and `old_func_id_map` in `TransformContext` instead
of cloning per function (~10–15 KB savings).

Phase 3: Move `GlslCompiler::new()` inside the per-function loop to force cleanup
each iteration. T::clone_one (Expr/Declaration AST clones) remains for future
investigation — likely from type/const resolution cloning through AST nodes.
