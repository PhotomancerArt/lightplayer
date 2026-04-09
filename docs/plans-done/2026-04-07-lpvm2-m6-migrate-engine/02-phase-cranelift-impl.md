# Phase 2: Implement CraneliftGraphics

## Scope

Create `CraneliftGraphics` implementing `LpGraphics`, and `CraneliftShader` implementing `LpShader`. Move the pixel-loop render logic from `ShaderRuntime` into `CraneliftShader::render()`.

## Files

```
lp-core/lp-engine/src/
├── gfx/
│   ├── cranelift.rs              # NEW: CraneliftGraphics + CraneliftShader
│   └── mod.rs                    # UPDATE: re-export cranelift module
```

## Implementation Details

### `cranelift.rs` structure

```rust
use alloc::boxed::Box;
use lp_shared::Texture;
use lpvm::VmContextHeader;
use lpvm_cranelift::{CraneliftEngine, CompileOptions, FloatMode, MemoryStrategy, DirectCall, JitModule};

use crate::error::Error;
use crate::gfx::{LpGraphics, LpShader, ShaderCompileOptions};

/// Cranelift-based graphics backend.
///
/// Wraps a `CraneliftEngine` and implements the `LpGraphics` trait.
/// The engine is created once and reused for all shader compilations.
pub struct CraneliftGraphics {
    engine: CraneliftEngine,
}

impl CraneliftGraphics {
    /// Create a new Cranelift graphics backend with default options.
    ///
    /// This is the constructor used by firmware crates.
    pub fn new() -> Self {
        // Always use Q32, default memory strategy, no max_errors (handled at call site)
        let options = lpvm_cranelift::CompileOptions {
            float_mode: FloatMode::Q32,
            q32_options: lps_q32::q32_options::Q32Options::default(),
            memory_strategy: MemoryStrategy::Default,
            max_errors: None, // We handle errors via ShaderCompileOptions
        };
        Self {
            engine: CraneliftEngine::new(options),
        }
    }
}

impl LpGraphics for CraneliftGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        // 1. Parse GLSL, lower to LPIR (via lps-frontend - already wired in lpvm_cranelift::jit)
        // 2. Compile via CraneliftEngine
        // 3. Extract DirectCall handle for 'render' entry point
        // 4. Return boxed CraneliftShader
        todo!("implement in phase")
    }
}

/// A shader compiled by Cranelift.
///
/// Holds the `CraneliftModule` (keeps JIT code alive) and optional
/// `DirectCall` handle for the render entry point.
struct CraneliftShader {
    /// The JIT module - must be kept alive as long as DirectCall is used.
    /// `DirectCall` points into this module's code.
    _module: lpvm_cranelift::CraneliftModule,
    /// DirectCall handle for the render() function.
    /// None if the shader has no render entry point.
    direct_call: Option<DirectCall>,
}

impl LpShader for CraneliftShader {
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), Error> {
        let dc = self.direct_call.as_ref().ok_or_else(|| Error::Other {
            message: alloc::format!("Shader has no render entry point"),
        })?;

        // Pixel loop copied from current ShaderRuntime::render_direct_call
        const Q32_SCALE: i32 = 65536;
        let time_q32 = (time * 65536.0 + 0.5) as i32;
        let output_size_q32 = [
            (texture.width() as i32) * Q32_SCALE,
            (texture.height() as i32) * Q32_SCALE,
        ];
        let vmctx = VmContextHeader::default();
        let vmctx_ptr = core::ptr::from_ref(&vmctx).cast::<u8>();

        for y in 0..texture.height() {
            for x in 0..texture.width() {
                let frag_coord_q32 = [
                    (x as i32) * Q32_SCALE,
                    (y as i32) * Q32_SCALE,
                ];
                let args = [
                    frag_coord_q32[0],
                    frag_coord_q32[1],
                    output_size_q32[0],
                    output_size_q32[1],
                    time_q32,
                ];
                let mut rgba_q32 = [0i32; 4];
                unsafe {
                    dc.call_i32_buf(vmctx_ptr, &args, &mut rgba_q32)
                        .map_err(|e| Error::Other {
                            message: alloc::format!("Shader direct call failed: {e}"),
                        })?;
                }

                // Q32 → u16 RGBA conversion (same as ShaderRuntime)
                let clamp_q32 = |v: i32| -> i32 { ... };
                let r = ((clamp_q32(rgba_q32[0]) as i64 * 65535) / Q32_SCALE as i64) as u16;
                let g = ((clamp_q32(rgba_q32[1]) as i64 * 65535) / Q32_SCALE as i64) as u16;
                let b = ((clamp_q32(rgba_q32[2]) as i64 * 65535) / Q32_SCALE as i64) as u16;
                let a = ((clamp_q32(rgba_q32[3]) as i64 * 65535) / Q32_SCALE as i64) as u16;

                texture.set_pixel_u16(x, y, [r, g, b, a]);
            }
        }
        Ok(())
    }

    fn has_render(&self) -> bool {
        self.direct_call.is_some()
    }
}
```

## Key Design Points

1. **CraneliftGraphics::new()** - Simple constructor firmware can call. Options are hardcoded (Q32, default memory) since that's what we use today.

2. **compile_shader implementation**:
   - Call `lps_frontend` to parse GLSL → LPIR
   - Call `CraneliftEngine::compile()` → `CraneliftModule`
   - Call `CraneliftModule::direct_call("render")` → `Option<DirectCall>`
   - Box and return `CraneliftShader { _module, direct_call }`

3. **Pixel loop** is copied from `ShaderRuntime::render_direct_call()` but lives inside `CraneliftShader::render()`.

4. **Error mapping**: `lpvm_cranelift` errors → `lp_engine::Error` (add variant if needed).

## Dependencies

Ensure `lp-engine/Cargo.toml` has:
- `lpvm-cranelift` (currently there)
- `lps-frontend` (probably needed for GLSL parsing)

## Validate

```bash
cargo check -p lp-engine --lib
```

No integration yet - just confirming the code compiles.
