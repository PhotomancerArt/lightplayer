# Summary — Q32 design doc + reference implementation

## Completed

- **`docs/design/q32.md`** — Canonical Q16.16 semantics: encoding, conversions,
  saturating arithmetic, div-by-zero (`0/0→0`, `±/0→±max`), `isnan`/`isinf`
  always false on Q32, `@unsupported` policy, backend conformance note.
- **`Q32` struct** (`q32.rs`) — Saturating `+`, `-`, `*`, `/`; div-by-zero rules;
  `mul_int` saturating; `abs` via `wrapping_abs`; fixed constant comments
  (PI, TAU, E, PHI); shared `sat_i64_const` helper.
- **`__lp_lpir_fdiv_q32`** — `0/0` returns `0` (was `MAX_FIXED`).
- **Unit tests** — Edge cases for saturation, div-by-zero, rem, constants,
  `from_fixed`, `to_i32` floor, `frac`, `to_u16_clamped`, etc.
- **Filetests** — `common-isinf.glsl` / `common-isnan.glsl`: file-level
  `@unsupported(float_mode=q32, …)`; removed stale `@unimplemented(backend=jit)`
  on first tests. New `scalar/float/q32-div-by-zero.glsl` with
  `@ignore(float_mode=f32)`.

## Deferred (per plan)

- WASM emitter audit.
- Naga → LPIR `isinf` lowering cleanup (milestone I / roadmap).
