# Stage IV summary

## Delivered

- **`lpir`:** `glsl_metadata` module (`GlslType`, qualifiers, param/function/module metadata).
- **`lp-glsl-naga`:** Lowering returns `(IrModule, GlslModuleMeta)`; `LowerError::InFunction`; params as `GlslParamMeta`.
- **`lp-glsl-wasm`:** Adjusted for metadata / param shape (if present in tree).
- **`lpir-cranelift`:** `CompilerError`; `JitModule` with `finalized_ptr*` / `func_names` / `glsl_meta`; `jit`, `jit_from_ir`, `jit_from_ir_owned`; `GlslQ32` + `JitModule::call`; `DirectCall` + `direct_call`; tests including GLSL `jit` + `call` and `call` vs `direct_call` for `float add(float,float)`.

## Validation (this pass)

- `cargo +nightly fmt -p lpir -p lp-glsl-naga -p lpir-cranelift`
- `cargo clippy -p lpir -p lp-glsl-naga -p lpir-cranelift -- -D warnings`
- `cargo test -p lpir -p lp-glsl-naga -p lpir-cranelift`

## Follow-ups

- Level-1 `call()`: `out` / `inout` marshalling (currently `Unsupported`).
- Hand-written LPIR via `jit_from_ir` has empty metadata; use `jit_from_ir_owned` or `jit()` for `call()`.
