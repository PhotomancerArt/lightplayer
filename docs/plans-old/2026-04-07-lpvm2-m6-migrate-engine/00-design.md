# M6: Migrate Engine — Design

## Scope

Decouple `lp-engine` from `lpvm-cranelift` by introducing an `LpGraphics` dyn trait that firmware crates inject at startup. No generics propagate through `LpServer` or `ProjectRuntime`. The pixel-loop hot path stays inside the concrete backend impl (one dyn call per frame, not per pixel).

## File Structure

```
lp-core/lp-engine/
├── Cargo.toml                              # UPDATE: make lpvm-cranelift optional (default feature)
├── src/
│   ├── lib.rs                              # UPDATE: export gfx module
│   ├── gfx/
│   │   ├── mod.rs                          # NEW: LpGraphics + LpShader traits
│   │   └── cranelift.rs                    # NEW: CraneliftGraphics impl
│   ├── nodes/shader/runtime.rs             # UPDATE: use dyn LpShader, accept Rc<dyn LpGraphics>
│   └── project/runtime.rs                  # UPDATE: hold Rc<dyn LpGraphics>, pass to ShaderRuntime

lp-core/lp-server/
├── src/
│   ├── server.rs                           # UPDATE: LpServer::new() takes Rc<dyn LpGraphics>
│   └── project.rs                          # UPDATE: pass graphics to ProjectRuntime

lp-fw/fw-esp32/
├── src/main.rs                             # UPDATE: create CraneliftGraphics, pass to LpServer

lp-fw/fw-emu/
├── src/main.rs                             # UPDATE: create CraneliftGraphics, pass to LpServer
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│ Firmware (fw-esp32 / fw-emu / future fw-wasm)                    │
│                                                                  │
│   let gfx: Rc<dyn LpGraphics> = Rc::new(CraneliftGraphics::new());
│   LpServer::new(gfx, output_provider, fs, ...)                  │
└──────────────┬───────────────────────────────────────────────────┘
               │ Rc<dyn LpGraphics>
               ▼
┌──────────────────────────────────────────────────────────────────┐
│ LpServer                                                         │
│   stores Rc<dyn LpGraphics>, passes to ProjectRuntime            │
└──────────────┬───────────────────────────────────────────────────┘
               │ Rc<dyn LpGraphics>
               ▼
┌──────────────────────────────────────────────────────────────────┐
│ ProjectRuntime                                                   │
│   stores Rc<dyn LpGraphics>, passes to ShaderRuntime             │
└──────────────┬───────────────────────────────────────────────────┘
               │ Rc<dyn LpGraphics>
               ▼
┌──────────────────────────────────────────────────────────────────┐
│ ShaderRuntime                                                    │
│   gfx.compile_shader(source, opts) → Box<dyn LpShader>          │
│   shader.render(texture, time)  ← one dyn call per frame        │
└──────────────┬───────────────────────────────────────────────────┘
               │ dyn LpShader::render()
               ▼
┌──────────────────────────────────────────────────────────────────┐
│ CraneliftShader (inside CraneliftGraphics)                       │
│   pixel loop with DirectCall::call_i32_buf() — zero overhead     │
│   (WasmShader would use LpvmInstance::call_q32() instead)        │
└──────────────────────────────────────────────────────────────────┘
```

## Trait Design

### `LpGraphics` (in `lp-engine/src/gfx/mod.rs`)

```rust
use crate::error::Error;
use lp_shared::Texture;

/// Compile options visible to lp-engine (backend-agnostic).
pub struct ShaderCompileOptions {
    pub q32_options: lps_q32::q32_options::Q32Options,
    pub max_errors: Option<usize>,
}

/// Graphics backend: compiles shaders, owns shared memory.
pub trait LpGraphics {
    /// Compile GLSL source into a runnable shader.
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error>;
}

/// A compiled, runnable shader.
pub trait LpShader: Send {
    /// Render the shader's `render()` entry point into a texture.
    /// Returns false if the shader has no render entry point.
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), Error>;

    /// Whether this shader has a render entry point.
    fn has_render(&self) -> bool;
}
```

### `CraneliftGraphics` (in `lp-engine/src/gfx/cranelift.rs`)

```rust
use lpvm_cranelift::{CraneliftEngine, CompileOptions, DirectCall};

pub struct CraneliftGraphics {
    engine: CraneliftEngine,
}

struct CraneliftShader {
    direct_call: Option<DirectCall>,
    // module kept alive so JIT code stays valid
    _module: CraneliftModule,
}

impl LpShader for CraneliftShader {
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), Error> {
        let dc = self.direct_call.as_ref().ok_or(...)?;
        // pixel loop with dc.call_i32_buf() — same code as current runtime.rs
    }
}
```

## Key Decisions

1. **No generics on LpServer/ProjectRuntime** — pure `Rc<dyn LpGraphics>` injection. One dyn call per shader per frame (rendering). Compilation is already slow enough that dyn overhead is invisible.

2. **Pixel loop stays inside the backend** — `CraneliftShader::render()` owns the pixel loop and uses `DirectCall` directly. Future `WasmShader::render()` will use `LpvmInstance::call_q32()` with its own loop. No per-pixel dyn overhead.

3. **`lpvm-cranelift` becomes optional on `lp-engine`** — behind a default `cranelift` feature. When `fw-wasm` comes, it won't pull in Cranelift.

4. **`ShaderCompileOptions` is backend-agnostic** — `lp-engine` doesn't know about `FloatMode` or `MemoryStrategy`. The backend maps these internally (Cranelift always uses Q32 today).

5. **Textures stay as-is for now** — `Texture` in `lp-shared` is a CPU buffer. Future M: `LpGraphics` gains `create_texture()` and textures live in shared/GPU memory.

6. **`gfx/` module in `lp-engine`** — extracted to `lp-gfx` crate later when GPU backends arrive.
