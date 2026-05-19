# Phase 2: Compute Shader API

## Scope Of Phase

Add the `lp-shader` serial compute compile and execution API.

In scope:

- Add `CompileComputeDesc`.
- Add `LpsComputeShader`.
- Add `LpsEngine::compile_compute_desc`.
- Add compute-specific no-reset tick execution path.
- Validate `tick()` existence and signature.
- Add shader-level tests for simple scalar/vector values and persistent
  globals.

Out of scope:

- `ComputeShaderDef` model validation beyond what is needed by this phase.
- Fluid emitter sentinel map tests.
- Engine node integration.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Tests go at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-shader/lp-shader/src/compile_compute_desc.rs`
- `lp-shader/lp-shader/src/compute_shader.rs`
- `lp-shader/lp-shader/src/compute_abi.rs`
- `lp-shader/lp-shader/src/engine.rs`
- `lp-shader/lp-shader/src/lib.rs`
- `lp-shader/lp-shader/src/tests.rs` or a compute-specific test module
- `lp-shader/lpvm/src/instance.rs`
- backend instance files from Phase 1

Expected API shape:

```rust
pub struct CompileComputeDesc<'a> {
    pub glsl: &'a str,
    pub compiler_config: CompilerConfig,
}

pub struct LpsComputeShader { ... }

impl LpsComputeShader {
    pub fn meta(&self) -> &LpsModuleSig;
    pub fn tick(&self, inputs: &[(&str, LpsValueF32)]) -> Result<(), LpsError>;
    pub fn get_output(&self, name: &str) -> Result<LpsValueF32, LpsError>;
}
```

Implementation notes:

1. `compile_compute_desc` should parse/lower GLSL similarly to
   `compile_px_desc`, but without render signature validation or render
   synthesis.

2. Require an authored zero-argument `void tick()`. If frontend parsing needs
   `main`, synthesize a wrapper:

   ```glsl
   void main() { tick(); }
   ```

   Do not silently accept a shader with no authored `tick()`.

3. `LpsComputeShader` should use a backend-erased adapter parallel to
   `LpsPxShader`.

4. Compute `tick()` must not reset globals each call. Current generic
   `LpvmInstance::call` and `call_q32` reset globals. Add a dedicated trait
   method if needed, for example:

   ```rust
   fn call_compute_tick(&mut self, fn_name: &str) -> Result<(), Self::Error>;
   ```

   It should validate/resolve the zero-param void function and call it without
   resetting globals.

5. `LpsComputeShader::tick` should:

   - set consumed inputs as uniforms using `set_uniform`;
   - call compute tick without resetting globals;
   - leave produced globals available to `get_output`.

6. `get_output(name)` reads a private global by name using Phase 1 global
   access.

Tests:

- A shader with `layout(binding = 0) uniform float x; float y; void tick() { y = x + 1.0; }`.
- A shader with `layout(binding = 0) uniform vec2 pos; vec2 out_pos;`.
- A shader with persistent internal global:

  ```glsl
  float count = 0.0;
  float out_count;
  void tick() { count += 1.0; out_count = count; }
  ```

  Two ticks should produce increasing values.

- Missing `tick()` and wrong `tick()` signatures should fail clearly.

## Validate

```bash
cargo fmt --check
cargo test -p lpvm
cargo test -p lp-shader
```

If frontend files are changed:

```bash
cargo test -p lps-frontend
```
