# GLSL WASM Implementation Notes

Issues and future work to track for the lps-wasm backend.

## Known Issues (tests ignored)

### Q32 float add/sub saturation

- **Status:** Implemented. Add and subtract use i32 overflow detection (a>0&&b>0&&result<0 → max, a<
  0&&b<0&&result>=0 → min). Nested expressions work via a `drop` after `local_tee(base+2)` so the
  overflow-check if/else replaces the raw sum rather than stacking on top of it.

### Q32 float multiplication

- **Basic `test_q32_float_mul`:** Passes (2×3 in Q16.16).
- **Saturation / range:** Large products (e.g. `1000.0 * 2000.0` clamped to max representable) can
  still diverge from the Cranelift emulator expectation in filetests — investigate `emit_q32_mul` /
  saturation vs reference.

### Break/continue block depth

- **Status:** Fixed for do_while, while, and for. Break and continue use `block_depth` tracking so
  `br` targets the correct block when inside nested structures (if, nested loops).
- **For loop continue:** For loops need a dedicated continue block around the body so `continue`
  branches to the block end (falls through to update) rather than the loop start (which would skip
  the update and infinite-loop).

## Phase 5 (Vectors) — Implemented

- LocalInfo + multi-local allocation
- Vector constructors (vec2/3/4, ivec*, uvec*, bvec*)
- Component access (`.x`, `.y`, `.z`, `.w`) and swizzle (`.xy`, `.rgba`, etc.)
- Vector variable load/store
- Vector arithmetic (vec+vec, scalar*vec, vec*scalar)
- Vector comparison (`==`, `!=`; aggregate, returns bool)
- Vector assignment, return, parameters

## Shared linear memory layout

The shader and builtins modules share a single host-owned `env.memory`. Compiler constants live in
`lps-wasm` `codegen/memory.rs`:

- **`LPFX_OUT_PARAM_BASE` (0):** LPFX result pointers and `out` vector scratch.
- **`LPFX_SCRATCH_BYTES` (64):** Reserved span (vec4-scale result + out vectors) for a single call
  site.

Written by builtins / shader codegen, read back into locals after each call. Safe to reuse across
sequential calls; unsafe for nested LPFX calls that need overlapping scratch without a bump.

This static-offset approach works for current patterns. It will need rethinking when:

- Multiple LPFX functions with out params are called in the same expression (nested calls) or
  results overlap scratch
- Texture data or uniform buffers are passed through shared memory
- The playground needs host→shader data transfer (e.g. mouse position, canvas size)
- Any form of concurrency or multi-instance execution is introduced

When that happens, consider: a compile-time bump allocator with per-function layout, reserved
regions for host data vs compiler scratch, or a proper memory map with documented segment
boundaries.

## Filetests (`wasm.q32`)

- **Linking:** `lps-filetests` can instantiate shader modules with `lps_builtins_wasm.wasm` and a
  single shared `env.memory` when imports require it.
- **Execution:** `WasmExecutable` implements vector returns via WASM multi-value (`call_vec`,
  `call_ivec`, etc.), matching exported component counts from `glsl_type_to_wasm_components`.
- **Coverage:** Many scalar and vector tests run; some files still fail against the Cranelift
  emulator baseline (mul saturation, nested min/max scratch, uvec paths, LPFX color helpers,
  `inout`, edge cases). Treat as a backlog, not a runner limitation.

## Optimizations (later)

### Pre-allocated temps overhead

- Every function gets 10 extra locals (2 broadcast + 8 vector-conv) even when it doesn't use vector
  constructors.
- **Idea:** Lazy allocation or analysis pass to add temps only when needed.
