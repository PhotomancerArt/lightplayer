# Streaming GLSL Compilation (Initial)

| Field | Value |
|-------|-------|
| **Date** | 2026-03-11 |
| **Trace** | traces/2026-03-11T17-03-25--examples-basic--streaming-glsl |
| **Baseline** | traces/2026-03-11T17-15-56--examples-basic--before-streaming-glsl |
| **Peak free (before)** | 99,422 B |
| **Peak free (after)** | 60,962 B |
| **Δ** | -38,460 B |
| **Allocs at peak (before)** | 1750 |
| **Allocs at peak (after)** | 2754 |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
Introduced `glsl_jit_streaming`: per-function compilation that generates CLIF IR, Q32-transforms, and compiles one function at a time, freeing each function's IR before the next. Replaces the batch pipeline that built all CLIF IR first.

## Effect
- Peak free: 99,422 B → 60,962 B (−38 KB). Significant regression.
- Cause: streaming path created more intermediate allocations (JITModule::declare_function, frontend::glsl_jit_streaming) before the per-function free. Declarations and metadata lived longer.

## Outcome
Kept structurally; performance recovered via subsequent optimizations (streaming-improvements, direct-q32, ast-free). The streaming design was correct; the initial implementation had allocation overhead that was later addressed.
