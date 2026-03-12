# Summary: Streaming Per-Function Compilation

## Overview

Successfully implemented a streaming per-function compilation pipeline that compiles GLSL functions one at a time, freeing each function's AST, CLIF IR, and codegen working set before starting the next. Goal: reduce peak heap usage on ESP32 by ~25-30 KB.

## Completed Work

### Phase 1: AST Node Counting

- Added `TypedFunction::ast_node_count()` with recursive counting of statement nodes
- Implemented helpers: `count_statement_nodes`, `count_simple_statement_nodes`, `count_selection_nodes`, `count_iteration_nodes`
- Added tests for simple and compound statements

### Phase 2: Streaming Pipeline Skeleton

- Extracted `build_target_for_jit()` for shared target creation
- Added `glsl_jit_streaming()` with two-module setup (float + Q32)
- Declarations in both modules, func_id_map/old_func_id_map, builtin mapping
- Sorted functions by AST node count
- Exported from crate

### Phase 3: Per-Function Transform Helper

- Added `transform_single_function()` in gl_module.rs
- Transforms one float Function to Q32 using existing Transform trait
- Added test verifying Q32 signature conversion

### Phase 4: Streaming Inner Loop

- Added `StreamingFuncInfo` struct for per-function metadata
- Implemented per-function loop: compile CLIF → transform → define → free
- Added verifier error handling for define_function (matching memory_optimized path)
- Added `compile_single_function_to_clif()` to GlslCompiler
- Added `build_jit_executable_streaming()` for finalize + pointer extraction
- Added correctness tests comparing streaming vs batch (test_streaming_returns_correct_value, test_streaming_multi_function_cross_calls)

### Phase 5: Wire Up ESP32 + Integration Test

- Updated `lp-core/lp-engine/src/nodes/shader/runtime.rs` to use `glsl_jit_streaming` instead of `glsl_jit`
- Created `lp-glsl/lp-glsl-compiler/tests/test_streaming_integration.rs`:
  - test_streaming_matches_batch_rainbow_shader (examples/basic rainbow shader)
  - test_streaming_matches_batch_multi_function (self-contained palette shader)
- Verified no_std compilation: `cargo check --no-default-features --features core`

### Phase 6: Cleanup & Validation

- No temporary code (TODOs, println!) added by this implementation
- All tests pass (compiler, engine, integration)
- Code formatted with cargo +nightly fmt

## Key Files

### New/Modified

- `lp-glsl/lp-glsl-compiler/src/frontend/semantic/mod.rs` - ast_node_count
- `lp-glsl/lp-glsl-compiler/src/frontend/mod.rs` - glsl_jit_streaming, build_target_for_jit
- `lp-glsl/lp-glsl-compiler/src/frontend/glsl_compiler.rs` - compile_single_function_to_clif
- `lp-glsl/lp-glsl-compiler/src/backend/module/gl_module.rs` - transform_single_function
- `lp-glsl/lp-glsl-compiler/src/backend/codegen/jit.rs` - build_jit_executable_streaming
- `lp-glsl/lp-glsl-compiler/src/lib.rs` - export glsl_jit_streaming
- `lp-glsl/lp-glsl-compiler/tests/test_streaming_integration.rs` - integration tests
- `lp-core/lp-engine/src/nodes/shader/runtime.rs` - ESP32 callsite

## Architecture

- Two modules: float (declarations only) for CLIF gen, Q32 for compilation
- Compilation order: ascending AST node count (smallest first)
- No unused metadata stored in Q32 module (source_text, source_map, etc.)
- Existing glsl_jit path preserved; glsl_jit_streaming is the new entry point for embedded
