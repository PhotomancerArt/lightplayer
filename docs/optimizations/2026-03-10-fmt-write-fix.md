# fmt::write Peak Reduction

| Field | Value |
|-------|-------|
| **Date** | 2026-03-10 |
| **Trace** | traces/2026-03-10T17-43-01--examples-basic--fmt-write-fix |
| **Baseline** | traces/2026-03-10T17-24-21--examples-basic--string-clone-fix-v2 |
| **Peak free (before)** | 69,526 B |
| **Peak free (after)** | 99,422 B |
| **Δ** | +29,896 B |
| **Allocs at peak (before)** | 2010 |
| **Allocs at peak (after)** | 1750 |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
Reduced peak from `fmt::write` / string formatting paths that were allocating large temporary buffers. Likely switched to `fmt::write` with pre-sized buffers or avoided unnecessary string clones during formatting.

## Effect
- Peak free: 69,526 B → 99,422 B (+30 KB).
- `fmt::write` and `format::format_inner` no longer dominate at peak.

## Outcome
Kept. This fix recovered significant headroom before the streaming GLSL work. The string-clone-fix traces showed regression; fmt-write-fix addressed a different hotspot.
