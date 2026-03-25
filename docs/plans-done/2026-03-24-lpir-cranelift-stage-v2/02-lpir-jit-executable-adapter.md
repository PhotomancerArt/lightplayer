## Scope of phase

**Prerequisite:** Phase **04** wired filetests to **`lp-glsl-exec`** /
**`lp-glsl-values`** (and **`lp-glsl-diagnostics`** for **`GlslError`**). Trait
definition lives in **`lp-glsl-exec`**, not **`lp-glsl-frontend`**.

Add **`lpir_jit_executable.rs`** implementing **`GlslExecutable`** for
**`lpir_cranelift::JitModule`** (Q32 primary).

- Map **`GlslValue`** arguments to **`GlslQ32`** for `call`.
- Map **`CallResult` / `GlslQ32`** returns to **`GlslValue`** or per-method
  results (`call_f32`, `call_i32`, `call_vec`, …) matching how **`WasmExecutable`**
  and legacy Cranelift executables behave in filetests.
- Implement **`get_function_signature`** / **`list_functions`** using
  **`GlslModuleMeta`** (or `JitModule` helpers) so run_detail can resolve overloads.

## Code organization reminders

- **`mod tests` first** in the new file; conversion helpers at bottom.
- Reuse existing filetests utilities (e.g. Q32 tolerance paths in execution) —
  do not fork assertion logic.

## Implementation details

- **`CompileOptions`:** `float_mode: FloatMode::Q32` for `jit.q32`; align with
  `target.float_mode` for future `jit.f32`.
- **Errors:** map **`lpir_cranelift::CompilerError`** / **`CallError`** into
  **`lp_glsl_diagnostics::GlslError`** with usable messages (pattern after
  **`WasmExecutable`**).
- **`format_emulator_state` / CLIF / VCode:** return `None` / empty for JIT
  unless cheap to forward from `lpir-cranelift` later.

## Tests

- Unit tests with **minimal GLSL** compiled through **`lpir_cranelift::jit`**:
  e.g. `float add(float a,float b){return a+b;}` then **`call_f32`** via adapter.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo test -p lp-glsl-filetests --lib
```

`cargo +nightly fmt`.
