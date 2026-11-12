# Future Work

Ideas that are out of scope for this plan but worth considering later.

## Eliminate the float module (~17 KB savings)

The streaming pipeline keeps a "float module" (`GlModule<JITModule>`) alive
during the entire compilation just for `declare_func_in_func` resolution during
CLIF IR generation. This module holds only declarations (no CLIF IR, no compiled
code) but still costs ~17 KB for JITModule metadata (ISA, declarations map,
memory provider, symbol lookup table).

This could be eliminated by refactoring `CodegenContext` to not require
`&mut GlModule<M>`. Instead, it would use a lightweight structure that holds:
- Function signatures (for building FuncRefs)
- Builtin function references
- Whatever `declare_func_in_func` needs

This would save the ~17 KB of float module overhead and simplify the two-module
architecture to a single module. The main cost is touching the codegen layer,
which currently threads `GlModule` through `CodegenContext` → `emit_statement` →
expression codegen → function call emission.

## Better function size estimation for compilation order

The streaming pipeline compiles smallest functions first to minimize peak memory.
Currently it uses a recursive AST node count as the heuristic. This could be
improved with a `SizeEstimate` trait that considers:
- Parameter and return type complexity (vectors/matrices expand to many CLIF values)
- Loop and branch depth (affects regalloc working set)
- Number of function calls (each generates call setup IR)
- Estimated CLIF IR instruction count based on expression complexity

The heuristic only affects compilation order, not correctness, so improvements
here are low-risk.

## Consolidate JIT compilation paths

After this plan, there are three JIT build paths:
- `build_jit_executable` — batch, no memory optimization
- `build_jit_executable_memory_optimized` — batch, frees CLIF IR after each function
- `glsl_jit_streaming` — streaming per-function pipeline

The streaming path is strictly better than `memory_optimized` for embedded use.
Once validated on ESP32, we should:
1. Replace `build_jit_executable_memory_optimized` with the streaming path
2. Consider whether `build_jit_executable` (non-optimized) is still needed or
   can also use the streaming path
3. Simplify the `memory_optimized` flag — ideally remove it and just have
   `glsl_jit` (host/tests) and `glsl_jit_streaming` (embedded), or unify them
