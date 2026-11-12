# Streaming GLSL Improvements — Round 2

## Context

After round 1 fixes (bypass GlModule::declare_function, borrow transform maps,
move GlslCompiler::new inside loop), streaming is at 244,134 bytes peak vs
228,258 bytes baseline (batch memory_optimized). Still 15,876 bytes worse.

## Why streaming is losing

The streaming approach trades per-function CLIF IR savings (~13 KB) for:

1. Two live JITModules at peak (+16,790) — the float module exists only for
   `declare_func_in_func` resolution during CLIF generation, but a full
   JITModule costs ~17 KB in declarations, ISA, memory provider, etc.

2. Streaming bookkeeping (+16,300) — `glsl_jit_streaming` maintains more
   HashMaps (func_id_map × 2, float_func_ids, glsl_signatures,
   cranelift_signatures) and sorted_functions. HashMap rehashes alone are
   6,488 bytes vs 660 in batch.

3. TypedShader alive at peak (~12 KB visible as T::clone_one) — in batch, the
   AST is dropped before compilation. In streaming, `sorted_functions` borrows
   `&TypedFunction` from the AST, keeping it alive through `define_function`.

## Plan

1. Lightweight float declarations (~17 KB savings) — replace float JITModule
2. Pre-allocate HashMaps (~6 KB savings) — avoid rehashes
3. Drop AST before compilation peak — restructure borrowing
4. Reduce StreamingFuncInfo overhead — smaller per-function bookkeeping

If all succeed, streaming should be ~10-20 KB better than batch baseline.
