# Plan D: Wire Up the Pipeline

Part of the direct-Q32 design (docs/designs/2026-03-11-direct-q32).
Depends on Plan B (Q32Strategy) and Plan C (builtin dispatch).

## Goal

Connect Q32Strategy to the compilation pipeline. When `decimal_format
== Q32`, the codegen emits Q32 IR directly — no float module, no
transform. The existing transform path is removed (not kept as fallback).

## Scope

| Path | Changes |
|------|---------|
| Batch JIT (`compile_glsl_to_gl_module_jit`) | Use Q32Strategy directly, remove `apply_transform` |
| Streaming JIT (`glsl_jit_streaming`) | Single module, no float module/transform/func_id_map |
| Object/emulator (`compile_glsl_to_gl_module_object`) | Same as batch, update CLIF capture |
| `SignatureBuilder` | Accept numeric mode, emit I32 directly for Q32 |
| `compile_function_to_clif_impl` | Accept `NumericMode` parameter |

## Phases

1. Make SignatureBuilder numeric-aware
2. Add NumericMode parameter to codegen functions
3. Update batch JIT path
4. Update object/emulator path
5. Simplify streaming JIT path
6. Tests + validation

## Key decisions (see questions.md)

- Replace transform path, no parallel fallback
- No feature flag
- SignatureBuilder emits correct types directly (not build-then-map)
- Existing `declare_builtins()` is sufficient
