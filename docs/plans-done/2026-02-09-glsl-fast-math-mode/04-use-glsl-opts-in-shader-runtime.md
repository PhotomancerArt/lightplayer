# Phase 4: Use glsl_opts in ShaderRuntime

## Scope of phase

Update `ShaderRuntime::compile_shader` to build `GlslOptions` from `config.glsl_opts` so that per-shader-node options (fast_math) flow into compilation.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Ensure config is available in compile_shader

`compile_shader` is called from `init` and `handle_fs_change`. At those points, `self.config` should already be set via `set_config`. Verify that `self.config` is `Some` when we need glsl_opts.

### 2. Build GlslOptions from config

**File**: `lp-core/lp-engine/src/nodes/shader/runtime.rs`

Current code:

```rust
let options = GlslOptions {
    run_mode: RunMode::HostJit,
    decimal_format: DecimalFormat::Q32,
};
```

Update to:

```rust
let fast_math = self.config
    .as_ref()
    .map(|c| c.glsl_opts.fast_math)
    .unwrap_or(false);

let options = GlslOptions {
    run_mode: RunMode::HostJit,
    decimal_format: DecimalFormat::Q32,
    fast_math,
};
```

Config is set via `set_config` before `compile_shader` is called (in init or handle_fs_change). `unwrap_or(false)` handles any edge case where config might not be set.

### 3. Handle ESP32 / object compilation path

Check if `ShaderRuntime` only uses `glsl_jit` or also has an object/emulator path. The search showed `glsl_jit` - that uses `compile_glsl_to_gl_module_jit` which we updated in phase 2. So we're done for the engine's JIT path.

If the engine uses emulator compilation anywhere, that path would use `compile_glsl_to_gl_module_object` which also takes `GlslOptions` - same flow.

### 4. Tests

- Ensure existing engine tests pass (they use default ShaderConfig, so fast_math=false).
- If there are shader runtime tests that construct ShaderConfig, add one that sets `glsl_opts: GlslOpts { fast_math: true }` and verifies compilation succeeds (no need to assert IR shape unless we have that infrastructure).

## Validate

```bash
cargo build -p lp-engine
cargo test -p lp-engine
```
