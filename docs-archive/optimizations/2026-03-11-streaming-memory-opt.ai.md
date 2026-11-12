# Streaming Memory Optimization

| Field | Value |
|-------|-------|
| **Date** | 2026-03-11 |
| **Trace** | traces/2026-03-11T21-12-24--examples-basic--streaming-memory-opt |
| **Baseline** | traces/2026-03-11T20-57-24--examples-basic--direct-q32 |
| **Peak free (before)** | 108,555 B |
| **Peak free (after)** | 113,294 B |
| **Δ** | +4,739 B |
| **Allocs at peak (before)** | 2154 |
| **Allocs at peak (after)** | 1978 |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
Streaming memory optimization (commit `932a3c3`): additional reductions in the per-function streaming loop. Likely reduced cloning of `func_id_map` / `old_func_id_map`, or moved `GlslCompiler::new()` inside the loop to force cleanup each iteration.

## Effect
- Peak free: 108,555 B → 113,294 B (+5 KB).
- Peak allocations: 214,386 B tracked at peak; `T::clone_one` and frontend allocations slightly reduced.

## Outcome
Kept. Incremental gain; the larger win came from ast-free-before-define.
