# LPFX Static Registry

| Field | Value |
|-------|-------|
| **Date** | 2026-03-12 |
| **Trace** | — |
| **Baseline** | traces/2026-03-12T10-40-46--examples-basic--chunkedmap-small |
| **Peak free (before)** | 160,418 B |
| **Peak free (after)** | — |
| **Δ** | — |
| **Allocs at peak (before)** | 1128 |
| **Allocs at peak (after)** | — |
| **Project** | 2026.01.21-03.01.12-test-project \| 10 frames |
| **Heap** | 320 KB |

---

## Change
Replaced heap-allocated LPFX function registry with a static `&[LpfnFn]` in ROM. `lpfn_fns::init_functions` previously allocated 143 `LpfnFn` (~4 KB) at first use; now uses `static LPFX_FNS` with `FunctionSignatureRef` and `ParameterRef` (`&'static str`, `&'static [ParameterRef]`). `find_lpfn_fn` refactored to loop-based lookup (no Vec allocs). Plan: docs/plans-done/2026-03-12-lpfn-fns-static. Commit: `e4139bd`.

## Effect
- Expected: eliminates 4,068 B + 143 allocations from `lpfn_fns::init_functions` at peak.
- Live allocations at end of trace should drop by that amount.
- No trace captured yet post-merge.

## Outcome
Kept. Implementation complete. Run `just profile` (or `cargo run -p lp-cli -- profile …`) to capture a trace and confirm the lpfn hotspot is gone.
