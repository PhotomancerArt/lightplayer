# Phase 1: Define LpGraphics + LpShader Traits

## Scope

Create the gfx module with backend-agnostic graphics traits. No backend implementations yet, no code changes to existing files.

## Files

```
lp-core/lp-engine/src/
├── gfx/
│   ├── mod.rs                    # NEW: module doc, re-exports
│   ├── lp_gfx.rs                 # NEW: LpGraphics trait
│   └── lp_shader.rs              # NEW: LpShader trait + ShaderCompileOptions
```

## Code Organization

- `mod.rs`: module-level documentation, `pub use` re-exports
- `lp_gfx.rs`: `LpGraphics` trait definition
- `lp_shader.rs`: `LpShader` trait, `ShaderCompileOptions`, related types

## Implementation Details

### `lp_shader.rs`

```rust
use crate::error::Error;
use lp_shared::Texture;

/// Backend-agnostic compile options for shader compilation.
///
/// This is the subset of options that `lp-engine` understands.
/// The concrete backend (CraneliftGraphics, WasmGraphics) maps these
/// to its internal compile options.
pub struct ShaderCompileOptions {
    /// Q32 arithmetic options (saturating/wrapping add/sub/mul/div).
    pub q32_options: lps_q32::q32_options::Q32Options,
    /// Maximum semantic errors to report from GLSL → LPIR front-end.
    pub max_errors: Option<usize>,
}

impl Default for ShaderCompileOptions {
    fn default() -> Self {
        Self {
            q32_options: lps_q32::q32_options::Q32Options::default(),
            max_errors: Some(20),
        }
    }
}

/// A compiled, runnable shader.
///
/// The concrete implementation (CraneliftShader, WasmShader) contains
/// the backend-specific handles and the pixel-loop render function.
pub trait LpShader: Send {
    /// Render the shader's `render()` entry point into a texture.
    ///
    /// Returns `Ok(())` on success, or an error if rendering fails
    /// (shader trap, out of fuel, etc).
    ///
    /// The texture is expected to be RGBA16 format. The shader receives
    /// frag_coord (in Q32), output_size (in Q32), and time (in Q32)
    /// as its three parameters.
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), Error>;

    /// Whether this shader has a render entry point.
    ///
    /// Shaders without `render(vec2, vec2, float) → vec4` cannot be
    /// rendered and should be called via other entry points (future API).
    fn has_render(&self) -> bool;
}
```

### `lp_gfx.rs`

```rust
use crate::error::Error;
use crate::gfx::lp_shader::{LpShader, ShaderCompileOptions};

/// Graphics backend: compiles shaders, owns shared memory.
///
/// Concrete implementations:
/// - `CraneliftGraphics` (lpvm-cranelift): JIT compile to native code
/// - Future: `WasmGraphics` (lpvm-wasm): compile to WASM, run in wasmtime/browser
/// - Future: `GpuGraphics`: compile to SPIR-V/MSL, run on GPU
///
/// The graphics backend is created by the firmware crate and injected
/// into `LpServer` at startup. It lives for the lifetime of the server.
pub trait LpGraphics: Send {
    /// Compile GLSL source into a runnable shader.
    ///
    /// Returns a boxed trait object that can be stored in `ShaderRuntime`.
    /// The shader holds the compiled artifact and any per-shader state
    /// (code pointers, instance handles, etc).
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error>;
}
```

### `mod.rs`

```rust
//! Graphics abstraction layer (`LpGraphics` / `LpShader`).
//!
//! This module provides the boundary between `lp-engine` and the
/// underlying graphics/shader execution backend.
//!
/// Concrete implementations live in sibling modules (e.g., `cranelift`).
/// Firmware crates create the concrete backend and inject it into
/// `LpServer` at startup.

pub mod lp_gfx;
pub mod lp_shader;

pub use lp_gfx::LpGraphics;
pub use lp_shader::{LpShader, ShaderCompileOptions};
```

## Dependencies

Add to `lp-engine/Cargo.toml`:
- `lps-q32` is already a dependency (for `q32_options`)

## Validate

```bash
cargo check -p lp-engine --lib
```

No warnings expected (new module, not yet used anywhere).
