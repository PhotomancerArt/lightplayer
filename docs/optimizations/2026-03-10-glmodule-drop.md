# GlModule Metadata Drop (Attempt)

| Field | Value |
|-------|-------|
| **Date** | 2026-03-10 |
| **Trace** | traces/2026-03-10T18-14-50--examples-basic--glmodule-drop |
| **Baseline** | traces/2026-03-10T18-12-15--examples-basic--glmodule-baseline |
| **Peak free (before)** | 79,620 B |
| **Peak free (after)** | 74,469 B |
| **Δ** | -5,151 B |
| **Allocs at peak (before)** | 1913 |
| **Allocs at peak (after)** | 1910 |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
Attempted to drop GlModule metadata (e.g. source_text, source_loc_manager, source_map, function_registry) early in the pipeline to reduce peak memory. The drop happened at a point where it shifted when peak occurred rather than reducing it.

## Effect
- Peak free: 79,620 B → 74,469 B (−5 KB). Regression.
- Peak occurred at a different instruction count; the drop may have changed allocation timing unfavorably.

## Outcome
Reverted. Do not drop GlModule metadata in the middle of the non-streaming path without a clear win. The streaming pipeline (docs/plans-done/2026-03-10-streaming-compilation) avoids storing this metadata in the Q32 module instead.
