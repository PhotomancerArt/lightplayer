# M6: Migrate Engine — Plan Notes

## Scope of Work

Make `lp-engine` backend-agnostic by introducing `LpGraphics` / `LpShader` dyn traits. Firmware crates inject the concrete graphics backend at startup. No generics propagate through `LpServer` or `ProjectRuntime`.

## Current State

### lp-engine (directly depends on lpvm-cranelift)

**`lp-core/lp-engine/src/nodes/shader/runtime.rs`:**
- Uses `lpvm_cranelift::{jit, JitModule, DirectCall, CompileOptions, FloatMode, ...}` directly
- `ShaderRuntime` stores `jit_module: Option<JitModule>` and `direct_call: Option<DirectCall>`
- `compile_shader()` calls `jit(glsl_source, &options)` directly
- `render_direct_call()` contains the pixel loop using `dc.call_i32_buf()`
- Maps `lp_model` Q32 options to `lpvm_cranelift` types

**`lp-core/lp-engine/Cargo.toml`:**
- Direct dependency: `lpvm-cranelift = { path = "...", features = ["glsl"] }`
- Feature flags proxy to `lpvm-cranelift`: `cranelift-optimizer`, `cranelift-verifier`
- `lpvm` trait crate already a dependency (used for `VmContextHeader`)

### lp-server (no engine coupling)

- `LpServer` holds `ProjectManager`, `base_fs`, `output_provider`
- No direct engine/backend types
- `Project` wraps `ProjectRuntime` from `lp-engine`

### ProjectRuntime (creates ShaderRuntime)

- Creates `ShaderRuntime::new(handle)` on line 351
- Does NOT hold engine references — coupling is entirely inside `ShaderRuntime`
- Also creates `TextureRuntime`, `FixtureRuntime`, `OutputRuntime`

### Firmware Crates

- `fw-emu` and `fw-esp32` depend on `lp-server`, which brings `lp-engine` + `lpvm-cranelift` transitively
- Neither firmware crate mentions engine types directly

## Resolved Questions

### Q1: How to handle the DirectCall hot path?

**Answer:** Pixel loop lives *inside* `CraneliftShader::render()`. The `dyn LpShader` boundary is per-frame, not per-pixel. Zero overhead on the hot path.

### Q2: Generic propagation strategy?

**Answer:** No generics. Pure `Rc<dyn LpGraphics>` injection from firmware → LpServer → ProjectRuntime → ShaderRuntime. One dyn call per shader per frame.

### Q3: Backend selection mechanism?

**Answer:** Firmware crate creates the concrete `CraneliftGraphics` and passes it as `Rc<dyn LpGraphics>`. No target detection or feature flags needed at the `lp-engine` level — the firmware knows what backend it wants.

### Q4: Where does the graphics abstraction live?

**Answer:** `lp-engine/src/gfx/` for now. `LpGraphics` + `LpShader` traits in `mod.rs`, `CraneliftGraphics` impl in `cranelift.rs`. Will extract to `lp-gfx` crate later when GPU backends arrive.

### Q5: What about textures?

**Answer:** Textures stay as-is (`lp_shared::Texture` CPU buffer) for M6. Next step adds texture management to `LpGraphics`. The trait is designed to accommodate this.

### Q6: What about lp-engine Cargo.toml features?

**Answer:** `lpvm-cranelift` becomes optional behind a default `cranelift` feature. The `cranelift-optimizer` and `cranelift-verifier` features require `cranelift`. When `fw-wasm` arrives, it won't enable the `cranelift` feature.

## Notes

- `ShaderCompileOptions` is the backend-agnostic subset of compile options. `CraneliftGraphics` internally maps to `lpvm_cranelift::CompileOptions` (always Q32, default memory strategy).
- `LpShader` is `Send` so it can be stored in `NodeRuntime` impls that require `Send`.
- The `LpGraphics` trait will grow: `create_texture()`, `free_texture()`, etc. But M6 only adds shader compile + render.
