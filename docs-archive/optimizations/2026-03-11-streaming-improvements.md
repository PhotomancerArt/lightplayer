# Streaming GLSL Improvements

| Field | Value |
|-------|-------|
| **Date** | 2026-03-11 |
| **Trace** | traces/2026-03-11T18-51-25--examples-basic--streaming-glsl-improvements2 |
| **Baseline** | traces/2026-03-11T17-03-25--examples-basic--streaming-glsl |
| **Peak free (before)** | 60,962 B |
| **Peak free (after)** | 84,966 B |
| **Δ** | +24,004 B |
| **Allocs at peak (before)** | 2754 |
| **Allocs at peak (after)** | 2663 |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
(From docs/plans-done/2026-03-10-streaming-compilation Follow-up)

1. Bypass `GlModule::declare_function` in streaming — use `module_mut_internal().declare_function()` to skip creating placeholder GlFunc entries (~22 KB savings).
2. Borrow `func_id_map` and `old_func_id_map` in `TransformContext` instead of cloning per function (~10–15 KB savings).

## Effect
- Peak free: 60,962 B → 84,966 B (+24 KB).
- JITModule::declare_function and frontend::glsl_jit_streaming allocations reduced.
- Recovers most of the loss from the initial streaming switch.

## Outcome
Kept. Essential for making streaming viable. Commits: `1060924`, `9b44f40` (additional streaming glsl improvements).
