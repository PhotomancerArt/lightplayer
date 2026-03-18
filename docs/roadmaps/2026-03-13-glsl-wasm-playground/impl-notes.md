# GLSL WASM Implementation Notes

Issues and future work to track for the lp-glsl-wasm backend.

## Known Issues (tests ignored)

### Q32 float multiplication
- **Test:** `test_q32_float_mul`
- **Problem:** WASM validation error — expected i32, found i64. The inline i64 sequence for Q32 mul triggers validation failure.
- **Needs:** Investigation of Q32 mul codegen and possible alternate approach.

### Break/continue block depth
- **Status:** Fixed for do_while, while, and for. Break and continue use `block_depth` tracking so `br` targets the correct block when inside nested structures (if, nested loops).
- **For loop continue:** For loops need a dedicated continue block around the body so `continue` branches to the block end (falls through to update) rather than the loop start (which would skip the update and infinite-loop).

## Phase 5 (Vectors) — Implemented

- LocalInfo + multi-local allocation
- Vector constructors (vec2/3/4, ivec*, uvec*, bvec*)
- Component access (`.x`, `.y`, `.z`, `.w`) and swizzle (`.xy`, `.rgba`, etc.)
- Vector variable load/store
- Vector arithmetic (vec+vec, scalar*vec, vec*scalar)
- Vector comparison (`==`, `!=`; aggregate, returns bool)
- Vector assignment, return, parameters

## Optimizations (later)

### Pre-allocated temps overhead
- Every function gets 10 extra locals (2 broadcast + 8 vector-conv) even when it doesn't use vector constructors.
- **Idea:** Lazy allocation or analysis pass to add temps only when needed.
