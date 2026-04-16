# Phase 3 — Implement LpsEngine + LpsFragShader

## Scope

Implement `LpsEngine<E>` with `compile_frag()` and `LpsFragShader<M>` with
`render_frame()`. This phase moves the GLSL compilation pipeline out of
consumers and into lp-shader. `render_frame` is a stub in M0 — the real
per-pixel loop comes in roadmap M2.

## Code organization reminders

- One concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom.
- Any temporary code should have a TODO comment.

## Implementation details

### `lp-shader/lp-shader/src/engine.rs`

```rust
use alloc::format;

use lpvm::LpvmEngine;
use lpvm::LpvmModule;
use lpvm::memory::{AllocError, LpvmMemory};
use lps_shared::TextureStorageFormat;

use crate::error::LpsError;
use crate::frag_shader::LpsFragShader;
use crate::texture_buf::LpsTextureBuf;

pub struct LpsEngine<E: LpvmEngine> {
    engine: E,
}

impl<E: LpvmEngine> LpsEngine<E> {
    pub fn new(engine: E) -> Self {
        Self { engine }
    }

    /// Compile GLSL source into a fragment shader.
    ///
    /// The output format is baked in at compile time — future milestones
    /// may use it for specialized code generation.
    pub fn compile_frag(
        &self,
        glsl: &str,
        output_format: TextureStorageFormat,
    ) -> Result<LpsFragShader<E::Module>, LpsError> {
        let naga = lps_frontend::compile(glsl)
            .map_err(|e| LpsError::Parse(format!("{e}")))?;
        let (ir, meta) = lps_frontend::lower(&naga)
            .map_err(|e| LpsError::Lower(format!("{e}")))?;
        drop(naga);
        let module = self.engine.compile(&ir, &meta)
            .map_err(|e| LpsError::Compile(format!("{e}")))?;
        LpsFragShader::new(module, meta, output_format)
    }

    /// Access the underlying engine (for advanced use / backend config).
    pub fn inner(&self) -> &E {
        &self.engine
    }
}
```

Note: `alloc_texture` is added in Phase 4 once `LpsTextureBuf` exists.

### `lp-shader/lp-shader/src/frag_shader.rs`

```rust
use alloc::format;

use lpvm::LpvmModule;
use lpvm::LpvmInstance;
use lps_shared::{LpsModuleSig, LpsValueF32, TextureStorageFormat};

use crate::error::LpsError;
use crate::texture_buf::LpsTextureBuf;

/// A compiled fragment shader with internal execution state.
///
/// Combines module + instance internally. Stateless from the consumer's
/// perspective — uniforms are passed into `render_frame`, not set as
/// mutable state.
pub struct LpsFragShader<M: LpvmModule> {
    module: M,
    instance: M::Instance,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
}

impl<M: LpvmModule> LpsFragShader<M> {
    /// Create a new fragment shader from a compiled module.
    pub(crate) fn new(
        module: M,
        meta: LpsModuleSig,
        output_format: TextureStorageFormat,
    ) -> Result<Self, LpsError> {
        let instance = module.instantiate()
            .map_err(|e| LpsError::Compile(format!("instantiate: {e}")))?;
        Ok(Self { module, instance, output_format, meta })
    }

    /// Module metadata (function signatures, uniform/global layouts).
    pub fn meta(&self) -> &LpsModuleSig {
        &self.meta
    }

    /// The output format this shader was compiled for.
    pub fn output_format(&self) -> TextureStorageFormat {
        self.output_format
    }

    /// Render one frame into the given texture buffer.
    ///
    /// `uniforms` should be an `LpsValueF32::Struct(...)` matching the
    /// shader's `meta().uniforms_type` layout.
    ///
    /// # M0 limitation
    ///
    /// This is a stub — it sets uniforms on the internal instance but does
    /// not run the per-pixel loop (that is roadmap M2). Returns `Ok(())`
    /// after setting uniforms successfully.
    pub fn render_frame(
        &mut self,
        uniforms: &LpsValueF32,
        _tex: &mut LpsTextureBuf,
    ) -> Result<(), LpsError> {
        // TODO(M2): per-pixel render loop
        self.apply_uniforms(uniforms)?;
        Ok(())
    }

    fn apply_uniforms(&mut self, uniforms: &LpsValueF32) -> Result<(), LpsError> {
        // Walk the uniforms struct and set each field on the instance.
        if let Some(ref uniforms_type) = self.meta.uniforms_type {
            if let LpsValueF32::Struct(fields) = uniforms {
                if let lps_shared::LpsType::Struct { members, .. } = uniforms_type {
                    for (member, value) in members.iter().zip(fields.iter()) {
                        self.instance
                            .set_uniform(&member.name, value)
                            .map_err(|e| LpsError::Render(format!("set uniform `{}`: {e}", member.name)))?;
                    }
                }
            }
        }
        Ok(())
    }
}
```

### `lp-shader/lp-shader/src/lib.rs` updates

Add the new modules:

```rust
mod engine;
mod frag_shader;
mod texture_buf; // empty for now, Phase 4 fills it in

pub use engine::LpsEngine;
pub use error::LpsError;
pub use frag_shader::LpsFragShader;
```

### Stub `texture_buf.rs`

Create a minimal file so `frag_shader.rs` can reference the type:

```rust
/// Texture buffer backed by LpvmMemory shared allocation.
///
/// Full implementation in Phase 4.
pub struct LpsTextureBuf {
    // TODO(phase-4): fields
}
```

## Validate

```bash
cargo check -p lp-shader
cargo test -p lp-shader
cargo check  # full default workspace
```
