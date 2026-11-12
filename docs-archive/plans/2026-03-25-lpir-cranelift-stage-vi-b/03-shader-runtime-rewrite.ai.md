# Phase 3: `ShaderRuntime` rewrite

## Scope

Replace old compiler types with `lpvm_cranelift::JitModule`, `DirectCall`, and
`jit()` + `CompileOptions`. Remove `dyn GlslExecutable` and the slow `call_vec`
path.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Field changes (`runtime.rs`)

Replace:

- `executable: Option<Box<dyn GlslExecutable + Send + Sync>>`
- `direct_func_ptr`, `direct_call_conv`, `direct_pointer_type`

With:

- `jit_module: Option<lpvm_cranelift::JitModule>` (name as you prefer; avoid
  shadowing `module` keyword)
- `direct_call: Option<lpvm_cranelift::DirectCall>`

Remove `FunctionPtr` wrapper if no longer needed.

Update all branches that clear `executable` / direct fields (file delete,
recompile, etc.).

### 2. `compile_shader`

- Build `CompileOptions`:
    - `float_mode: lpir::FloatMode::Q32` (re-exported as `lpvm_cranelift::FloatMode`
      if needed)
    - `q32_options`: map from `self.config.as_ref().map(|c| c.glsl_opts)` using
      small helpers (`AddSubMode`, `MulMode`, `DivMode` in `lp-model` →
      `lpvm_cranelift` enums)
    - `memory_strategy`: `LowMemory` on embedded-style builds if you have a cfg;
      otherwise match old `GlslOptions::default_memory_optimized()` behavior
    - `max_errors`: port `DEFAULT_MAX_ERRORS` constant into `lp-engine` or use a
      literal consistent with old crate

- Drop old executable before compile (keep OOM-avoidance comment).

- Call `lpvm_cranelift::jit(glsl_source, &options)` → `Result<JitModule,
  CompilerError>`.

- On success: `self.direct_call = jit_module.direct_call("main")` (handle
  `None`: clear direct path, optionally log).

- `panic-recovery`: wrap `jit()` the same way as old `catch_unwind` around
  `glsl_jit_streaming`; map panic to a stable user-facing string (no
  `GlslDiagnostics` type — use `String` or `Error::Other`).

- On error: `format!("{e}")` for `compilation_error` / state (Q4 decision).

### 3. `render`

- Require `self.direct_call.as_ref()` for the fast path. If missing, return
  `Error::Other` with a clear message (shader compiled but no `main` direct
  call — should not happen for standard shaders).

- Replace `render_direct_call` body:
    - Stack buffer `let mut rgba_q32 = [0i32; 4];`
    - Args as `[i32; 5]` for frag coord (2), output size (2), time (1), all Q32
      scaled like today
    - `unsafe { dc.call_i32_buf(&args, &mut rgba_q32)? }` — map `CallError` to
      `Error::Other`

- Remove the `else` branch that used `executable.call_vec` and
  `lps_cranelift::GlslValue`.

### 4. Imports

Remove: `lps_cranelift`, `lps_jit_util`, `cranelift_codegen`.

Add: `lpvm_cranelift` types as needed.

### 5. Tests

- Existing `test_shader_runtime_creation` should still compile.
- Add a test that compiles minimal GLSL and checks `direct_call` is `Some` if
  feasible without a full `NodeInitContext` (optional; may defer to integration).

## Validate

```bash
cargo test -p lp-engine
cargo clippy -p lp-engine --all-features -- -D warnings
```
