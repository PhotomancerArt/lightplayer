# Implementation Notes

## Completed

### Phase 2: Pre-allocate HashMaps
- Pre-allocate `func_id_map`, `old_func_id_map`, `float_func_ids`, `jit_func_id_map`, `sorted_functions`, `collected_signatures` with known capacity
- Capacity = `num_functions + num_builtins` for maps that include builtins
- Avoids rehashes during incremental growth (~5-6 KB savings expected)

### Phase 4: Reduce Streaming Bookkeeping
- **4A**: Removed `float_sig` from `StreamingFuncInfo` — signature is recomputed inside `compile_single_function_to_clif` from `TypedFunction`
- **4C**: Build `jit_func_id_map` during declaration loop instead of separate pass
- **4D**: Defer `glsl_signatures` and `cranelift_signatures` population to after the define loop — collect in `Vec` during loop, populate HashMaps after `drop(float_module)`, so HashMap structures are not allocated during compilation peak

## Deferred

### Phase 1: Lightweight Float Declarations
Would require `ModuleContext` trait, `DeclarationsOnlyModule`, and changing `CodegenContext` generic from `M: Module` to `C: ModuleContext` across ~15 codegen files. Medium risk, high impact (~17 KB savings).

### Phase 3: Drop AST Before Compilation Peak
Semi-streaming approach (two-pass: generate all CLIF then define) trades AST+float_module for holding all Q32 CLIF IR at once. For the 11-function test workload with small AST, may not yield net savings. Would matter more for larger shaders.
