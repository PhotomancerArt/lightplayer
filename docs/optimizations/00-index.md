# Optimization Log Index

Quick reference for memory optimizations. Trace summaries use peak free bytes (higher = better).

| Date | Name | Δ Peak Free | Δ Allocs | Outcome |
|------|------|-------------|----------|---------|
| 2026-03-10 | [chunkedvec-12](2026-03-10-chunkedvec-12.md) | +10 KB | -329 | Kept |
| 2026-03-10 | [fmt-write-fix](2026-03-10-fmt-write-fix.md) | +30 KB | -260 | Kept |
| 2026-03-10 | [glmodule-drop](2026-03-10-glmodule-drop.md) | -5 KB | -3 | Reverted |
| 2026-03-11 | [streaming-glsl](2026-03-11-streaming-glsl.md) | -38 KB | +1004 | Recovered via improvements |
| 2026-03-11 | [streaming-improvements](2026-03-11-streaming-improvements.md) | +24 KB | -91 | Kept |
| 2026-03-11 | [direct-q32](2026-03-11-direct-q32.md) | +24 KB | -509 | Kept |
| 2026-03-11 | [streaming-memory-opt](2026-03-11-streaming-memory-opt.md) | +5 KB | -176 | Kept |
| 2026-03-11 | [ast-free-before-define](2026-03-11-ast-free-before-define.md) | +28 KB | -511 | Kept |
| 2026-03-12 | [chunkedvec-dynamic](2026-03-12-chunkedvec-dynamic.md) | +20 KB | -339 | Kept |
| 2026-03-12 | [chunkedmap-small](2026-03-12-chunkedmap-small.md) | 0 | 0 | Kept (consolidation) |
| 2026-03-12 | [lpfx-static-registry](2026-03-12-lpfx-static-registry.md) | — | — | Kept (no trace yet) |

## Baseline progression

| Trace | Peak Free | Allocs |
|-------|-----------|--------|
| chunkedvec-8 (early) | 85 KB | 1979 |
| chunkedvec-12 | 95 KB | 1650 |
| fmt-write-fix (non-streaming) | 99 KB | 1750 |
| glmodule-baseline | 80 KB | 1913 |
| streaming-glsl (initial) | 61 KB | 2754 |
| streaming-improvements | 84 KB | 2663 |
| direct-q32 | 109 KB | 2154 |
| streaming-memory-opt | 113 KB | 1978 |
| ast-free | 141 KB | 1467 |
| chunkedvec-dynamic | 160 KB | 1128 |
| chunkedmap-small | 160 KB | 1128 |

## Attempts that regressed (do not retry)

| Trace | Peak Free | Allocs | Note |
|-------|-----------|--------|------|
| string-clone-fix | 69 KB | ~2010 | Regressed vs chunkedvec-12; reverted or superseded |
| shrink-to-fit | 65 KB | — | Regressed; reverted |
