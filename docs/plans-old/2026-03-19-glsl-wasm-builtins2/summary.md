# Summary: GLSL → WASM builtins (continuation)

## Shipped

- **psrdnoise seed:** `uint seed` on `lpfn_psrdnoise` overloads end-to-end (frontend LPFX sigs,
  Cranelift builtin registry arity, Rust `extern "C"` wrappers + GLSL strings in builtins,
  regenerated `builtin_wasm_import_types` / mapping). Example shaders updated with `0u` where
  needed.
- **Q32 math:** `floor` and `fract` inlined in the WASM backend (`builtin_inline.rs`); trig/exp
  family covered by compile or link tests as planned.
- **LPFX WASM calls:** `lpfn_call.rs` resolves overloads, flattens `In` args, passes `out` as `i32`
  offsets into shared memory, calls imported builtins, loads scalar/vector results from scratch;
  `memory.rs` documents `LPFX_OUT_PARAM_BASE` / `LPFX_SCRATCH_BYTES`.
- **Q32 context fixes:** Float vectors use i32 temps/locals (broadcast, conversion, binary paths) so
  wasmtime does not see i32/f32 local mismatches.
- **Filetests:** `wasm_link` loads `lps_builtins_wasm.wasm`, shared `env.memory`, wasmtime linker;
  `WasmExecutable` implements `call_vec` / `call_*vec` via multi-value returns matching
  `glsl_type_to_wasm_components`.
- **Rainbow:** `main` compiles, links, and runs under wasmtime in `q32_builtin_link` (with
  `CYCLE_PALETTE` const workaround for the single-file `glsl_wasm` path).
- **Q32 `abs`:** Fixed `emit_i32_abs_from_stack` comparison operand order so `abs()` cannot return
  negative fixed-point values.

## Known limitations

- **`glsl_wasm` vs batch path:** Module-scope `const` (e.g. `CYCLE_PALETTE`) is not handled the same
  as the Cranelift batch pipeline; Rainbow test strips/replaces that pattern.
- **Linear memory:** Static scratch at byte 0 is sufficient for current LPFX patterns; nested LPFX
  with multiple `out` regions in one expression or host buffers will need a richer layout.
- **`wasm.q32` filetests:** The runner links builtins and runs many tests; a **subset of files still
  fail** on real codegen/semantic gaps (e.g. large `float * float` saturation vs emulator
  expectation, nested `min`/`max` scratch, some `uvec` conversions, LPFX color helpers, `inout`,
  edge domains). These are **tracked for follow-up work**, not this plan closure.
- **Matrices:** `call_mat` in the WASM runner still errors; mat returns remain out of scope for this
  plan.

## Follow-ups

- Grow/document memory map (scratch bump, host segments, alignment).
- Browser playground + wasm packaging.
- Matrix multi-return or struct-return strategy for WASM if filetests require it.
- Align `glsl_wasm` const/global resolution with the Cranelift frontend.
- Green `wasm.q32` filetests by fixing remaining Q32 builtins and scratch lifetime bugs.
