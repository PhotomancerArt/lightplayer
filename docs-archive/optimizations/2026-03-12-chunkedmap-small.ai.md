# ChunkedHashMap Small Initial Capacity

| Field | Value |
|-------|-------|
| **Date** | 2026-03-12 |
| **Trace** | traces/2026-03-12T10-40-46--examples-basic--chunkedmap-small |
| **Baseline** | traces/2026-03-12T09-59-46--examples-basic--chunkedvec-dynamic |
| **Peak free (before)** | 160,418 B |
| **Peak free (after)** | 160,418 B |
| **Δ** | 0 B |
| **Allocs at peak (before)** | 1128 |
| **Allocs at peak (after)** | 1128 |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
Reduced ChunkedHashMap initial capacity / bucket size so small maps (e.g. a few functions, symbol tables) do not over-allocate. Consolidation of chunked collections tuning.

## Effect
- Peak free: unchanged at 160,418 B.
- No regression; allocation patterns similar. `RawTable::reserve_rehash` and ChunkedHashMap allocations slightly different but net neutral.

## Outcome
Kept. Consolidation pass; maintains the gains from chunkedvec-dynamic. Future work: lpfn static registry to eliminate the 143-init allocation hotspot.
