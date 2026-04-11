# Phase 3: Migrate ShaderRuntime to LpShader

## Scope

Replace `ShaderRuntime`'s direct `lpvm-cranelift` usage with `Rc<dyn LpGraphics>` injection and `Box<dyn LpShader>` storage.

## Files

```
lp-core/lp-engine/src/
├── nodes/shader/runtime.rs       # UPDATE: drop JitModule/DirectCall, use LpShader
```

## Current ShaderRuntime Structure

```rust
pub struct ShaderRuntime {
    config: Option<ShaderConfig>,
    jit_module: Option<JitModule>,          // REMOVE
    direct_call: Option<DirectCall>,        // REMOVE
    texture_handle: Option<TextureHandle>,
    compilation_error: Option<String>,
    state: ShaderState,
    node_handle: NodeHandle,
    render_order: i32,
}
```

## New ShaderRuntime Structure

```rust
use alloc::rc::Rc;
use crate::gfx::{LpGraphics, LpShader, ShaderCompileOptions};

pub struct ShaderRuntime {
    config: Option<ShaderConfig>,
    graphics: Rc<dyn LpGraphics>,           // NEW: injected at creation
    shader: Option<Box<dyn LpShader>>,      // NEW: compiled shader
    texture_handle: Option<TextureHandle>,
    compilation_error: Option<String>,
    state: ShaderState,
    node_handle: NodeHandle,
    render_order: i32,
}
```

## Constructor Changes

```rust
impl ShaderRuntime {
    /// Create a new ShaderRuntime with injected graphics backend.
    ///
    /// The graphics backend is passed as Rc<dyn LpGraphics> and cloned
    /// into the runtime for shader compilation.
    pub fn new(node_handle: NodeHandle, graphics: Rc<dyn LpGraphics>) -> Self {
        Self {
            config: None,
            graphics,                              // NEW
            shader: None,                           // NEW
            texture_handle: None,
            compilation_error: None,
            state: ShaderState::new(FrameId::default()),
            node_handle,
            render_order: 0,
        }
    }
}
```

## Method Changes

### compile_shader() rewrite

Replace the entire `compile_shader()` method:

```rust
fn compile_shader(&mut self, glsl_source: &str) -> Result<(), Error> {
    log::info!("Shader {} compilation starting", self.node_handle.as_i32());

    // Build backend-agnostic options from config
    let q32_options = self
        .config
        .as_ref()
        .map(|c| map_q32_options(&c.glsl_opts))
        .unwrap_or_default();

    let compile_opts = ShaderCompileOptions {
        q32_options,
        max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
    };

    // Clear old state
    self.shader = None;
    self.compilation_error = None;

    // Compile via graphics backend
    #[cfg(feature = "panic-recovery")]
    let compile_result: Result<Box<dyn LpShader>, String> =
        match catch_unwind(AssertUnwindSafe(|| {
            self.graphics.compile_shader(glsl_source, &compile_opts)
        })) {
            Ok(inner) => inner.map_err(|e| format!("{e}")),
            Err(_) => Err(String::from("OOM during shader compilation")),
        };

    #[cfg(not(feature = "panic-recovery"))]
    let compile_result: Result<Box<dyn LpShader>, String> =
        self.graphics.compile_shader(glsl_source, &compile_opts)
            .map_err(|e| format!("{e}"));

    match compile_result {
        Ok(shader) => {
            self.shader = Some(shader);
            self.compilation_error = None;
            let frame_id = FrameId::default();
            self.state.error.set(frame_id, None);
            Ok(())
        }
        Err(e) => {
            self.shader = None;
            self.compilation_error = Some(e.clone());
            let frame_id = FrameId::default();
            self.state.error.set(frame_id, Some(e.clone()));
            log::warn!("Shader {} compilation failed: {}", self.node_handle.as_i32(), e);
            Err(Error::InvalidConfig {
                node_path: format!("shader-{}", self.node_handle.as_i32()),
                reason: format!("GLSL compilation failed: {e}"),
            })
        }
    }
}
```

### render() rewrite

```rust
fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
    let texture_handle = self.texture_handle.ok_or_else(|| Error::Other {
        message: String::from("Texture handle not resolved"),
    })?;

    let shader = self.shader.as_mut().ok_or_else(|| Error::Other {
        message: String::from(
            "Shader has no compiled shader (compilation may have failed or shed occurred)"
        ),
    })?;

    if !shader.has_render() {
        return Err(Error::Other {
            message: String::from("Shader has no render() entry point"),
        });
    }

    let time = ctx.get_time();
    let texture = ctx.get_texture_mut(texture_handle)?;

    shader.render(texture, time).map_err(|e| Error::Other {
        message: format!("Shader render failed: {e}"),
    })
}
```

### shed_optional_buffers() update

```rust
fn shed_optional_buffers(...) -> Result<(), Error> {
    self.shader = None;  // Changed from jit_module/direct_call
    // Keep glsl_code for debug UI
    Ok(())
}
```

### handle_fs_change() update

Update the Delete branch:

```rust
ChangeType::Delete => {
    self.shader = None;  // Changed from jit_module/direct_call
    let error_msg = "GLSL file deleted".to_string();
    // ... rest unchanged
}
```

## Helper function

```rust
/// Map lp_model Q32 options to lps_q32 Q32Options.
fn map_q32_options(opts: &lp_model::glsl_opts::GlslOpts) -> lps_q32::q32_options::Q32Options {
    use lp_model::glsl_opts::{AddSubMode, MulMode, DivMode};
    lps_q32::q32_options::Q32Options {
        add_sub: match opts.add_sub {
            AddSubMode::Saturating => lps_q32::q32_options::AddSubMode::Saturating,
            AddSubMode::Wrapping => lps_q32::q32_options::AddSubMode::Wrapping,
        },
        mul: match opts.mul {
            MulMode::Saturating => lps_q32::q32_options::MulMode::Saturating,
            MulMode::Wrapping => lps_q32::q32_options::MulMode::Wrapping,
        },
        div: match opts.div {
            DivMode::Saturating => lps_q32::q32_options::DivMode::Saturating,
            DivMode::Reciprocal => lps_q32::q32_options::DivMode::Reciprocal,
        },
    }
}
```

## Remove old code

Delete:
- `render_direct_call()` method (moved to CraneliftShader)
- Direct imports of `lpvm_cranelift` types
- `jit_module` and `direct_call` fields

## Validate

```bash
cargo check -p lp-engine --lib
```

Expect errors about `ShaderRuntime::new()` call sites (need to pass graphics) — fixed in next phase.
