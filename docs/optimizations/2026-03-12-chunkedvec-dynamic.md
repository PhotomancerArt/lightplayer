# ChunkedVec Dynamic Chunk Sizing

| Field | Value |
|-------|-------|
| **Date** | 2026-03-12 |
| **Trace** | traces/2026-03-12T09-59-46--examples-basic--chunkedvec-dynamic |
| **Baseline** | traces/2026-03-11T21-28-06--examples-basic--ast-free |
| **Peak free (before)** | 140,918 B |
| **Peak free (after)** | 160,418 B |
| **Δ** | +19,500 B |
| **Allocs at peak (before)** | 1467 |
| **Allocs at peak (after)** | 1128 |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
Switched ChunkedVec from fixed chunk size (12) to dynamic chunk sizing based on expected growth or use-case. Reduces over-allocation for small collections and improves fit for codegen/cranelift usage patterns.

## Effect
- Peak free: 140,918 B → 160,418 B (+20 KB).
- ChunkedVec and related growth paths (RawVecInner, ChunkedHashMap) better match actual usage.
- Peak: 167,262 B tracked, 1128 allocs.

## Outcome
Kept. Best peak so far. Dynamic sizing proved superior to fixed chunk size for this workload.
