# AST Free Before define_function

| Field | Value |
|-------|-------|
| **Date** | 2026-03-11 |
| **Trace** | traces/2026-03-11T21-28-06--examples-basic--ast-free |
| **Baseline** | traces/2026-03-11T21-12-24--examples-basic--streaming-memory-opt |
| **Peak free (before)** | 113,294 B |
| **Peak free (after)** | 140,918 B |
| **Δ** | +27,624 B |
| **Allocs at peak (before)** | 1978 |
| **Allocs at peak (after)** | 1467 |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
Free each function's AST bodies before calling `define_function` in `glsl_jit_streaming`. AST was previously held until the end of the per-function loop; now released immediately after CLIF IR generation. Commit: `b06226d` (perf(glsl): free AST bodies before define_function in streaming).

## Effect
- Peak free: 113,294 B → 140,918 B (+28 KB). Largest single gain in the streaming pipeline.
- Peak tracked: 214,386 B → 186,762 B.
- `T::clone_one` and `frontend::glsl_jit_streaming` shrink at peak; ChunkedVec/ChunkedHashMap dominate.

## Outcome
Kept. AST bodies are short-lived; freeing before codegen avoids holding multiple functions' IR + AST in memory. No regressions.
