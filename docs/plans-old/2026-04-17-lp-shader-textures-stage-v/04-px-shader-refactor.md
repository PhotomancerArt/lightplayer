# Phase 4 — `LpsPxShader` refactor + `render_frame` wiring

## Scope

Drop the `<M: LpvmModule>` generic from `LpsPxShader` by hiding the
backend type behind `Box<dyn PxShaderBackend>`. Wire
`LpsEngine::compile_px` to invoke the Phase 3 synth before the
backend's `compile()`, and wire `render_frame` to call the Phase 2
trait method. Add warmup-time validation in `LpsPxShader::new` that
the synthesised function exists with the expected signature.

After this phase, `cargo build --workspace --all-features` succeeds
end-to-end and `render_frame` *calls* into the synthesised loop on
every backend that has a Phase 2 impl.

Closes Q5 (and folds the warmup-validation question that arose with
the single-method trait shape).

## Prerequisites

- Phase 1 (`LpsFnKind`) merged.
- Phase 2 (`call_render_texture` on every backend) merged.
- Phase 3 (synth) merged.

## Code organisation reminders

- `LpsPxShader` lives in
  [`lp-shader/lp-shader/src/px_shader.rs`](../../../lp-shader/lp-shader/src/px_shader.rs).
- `LpsEngine::compile_px` lives in
  [`lp-shader/lp-shader/src/engine.rs`](../../../lp-shader/lp-shader/src/engine.rs).
- The `lp-shader` crate is `no_std + alloc`; `Box<dyn>` is fine.
- Existing tests
  ([`lp-shader/src/tests.rs`](../../../lp-shader/lp-shader/src/tests.rs):
  `render_frame_no_uniforms`, `render_frame_sets_uniforms`) must
  continue to pass; they exercise the uniforms path which is
  unchanged.
- Pre-existing `LpsPxShader<E::Module>` consumers in this crate
  (engine.rs, tests.rs) get updated; no external consumers exist
  (verified via grep).

## Implementation details

### `lp-shader/lp-shader/src/px_shader.rs` — type-erase the inner

Replace the generic struct with a trait object, and add a small
backend-specific adapter that implements the trait.

```rust
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use core::cell::RefCell;

use lps_shared::{LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, LpsValueF32, TextureStorageFormat};
use lpvm::{LpvmBuffer, LpvmInstance, LpvmModule};

use crate::error::LpsError;
use crate::texture_buf::LpsTextureBuf;

/// Backend-erased operations on a compiled pixel shader's runtime instance.
pub trait PxShaderBackend {
    fn call_render_texture(
        &mut self,
        name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), LpsError>;

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), LpsError>;
}

/// Adapter erasing a concrete `(M: LpvmModule, M::Instance)` pair behind
/// `PxShaderBackend`. Owns both: the module is retained for the lifetime
/// of the instance (compiled code may be referenced by the instance).
struct BackendAdapter<M: LpvmModule> {
    #[allow(dead_code, reason = "retain compiled module for instance lifetime")]
    module: M,
    instance: M::Instance,
}

impl<M: LpvmModule + 'static> PxShaderBackend for BackendAdapter<M> {
    fn call_render_texture(
        &mut self,
        name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), LpsError> {
        self.instance
            .call_render_texture(name, texture, width, height)
            .map_err(|e| LpsError::Render(format!("call_render_texture `{name}`: {e}")))
    }

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), LpsError> {
        self.instance
            .set_uniform(path, value)
            .map_err(|e| LpsError::Render(format!("set_uniform `{path}`: {e}")))
    }
}

/// A compiled pixel shader with internal execution state.
///
/// Holds its backend instance behind `Box<dyn PxShaderBackend>` so the
/// public type is monomorphic. `render_frame` runs the per-pixel loop
/// inside the synthesised `__render_texture_<format>` function via the
/// backend's `LpvmInstance::call_render_texture` fast path.
///
/// The instance lives in a [`RefCell`] so [`Self::render_frame`] can
/// take `&self`; mutation goes through runtime borrow checks (panic
/// if re-entrant). This type is `!Sync` as a result.
pub struct LpsPxShader {
    inner: RefCell<Box<dyn PxShaderBackend>>,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
    /// Format-specific synthesised entry, e.g. `"__render_texture_rgba16"`.
    render_texture_fn_name: String,
    /// Index of `render` in `meta.functions` (preserved from compile_px).
    render_fn_index: usize,
}

impl LpsPxShader {
    /// Construct from a backend-typed module + the synthesised metadata.
    ///
    /// Validates that the synthesised render-texture function exists
    /// in `meta` with the expected signature shape before accepting.
    pub(crate) fn new<M: LpvmModule + 'static>(
        module: M,
        meta: LpsModuleSig,
        output_format: TextureStorageFormat,
        render_fn_index: usize,
        render_texture_fn_name: String,
    ) -> Result<Self, LpsError> {
        // Warmup-time validation: the synthesised function must be in meta.
        let synth_sig = meta.functions.iter().find(|f| f.name == render_texture_fn_name)
            .ok_or_else(|| LpsError::Compile(format!(
                "compile_px: synthesised function `{render_texture_fn_name}` missing from meta"
            )))?;
        if synth_sig.kind != LpsFnKind::Synthetic {
            return Err(LpsError::Compile(format!(
                "compile_px: function `{render_texture_fn_name}` is not marked Synthetic"
            )));
        }
        // Surface signature shape: parameters length == 3, return type Void.
        // (The rigorous shape check — IrType::Pointer on param 0 — runs
        //  inside LpvmInstance::call_render_texture on first invocation.)
        if synth_sig.return_type != LpsType::Void {
            return Err(LpsError::Compile(format!(
                "compile_px: `{render_texture_fn_name}` must return void"
            )));
        }
        if synth_sig.parameters.len() != 3 {
            return Err(LpsError::Compile(format!(
                "compile_px: `{render_texture_fn_name}` must take 3 parameters, found {}",
                synth_sig.parameters.len()
            )));
        }

        let instance = module.instantiate()
            .map_err(|e| LpsError::Compile(format!("instantiate: {e}")))?;
        let inner: Box<dyn PxShaderBackend> = Box::new(BackendAdapter { module, instance });

        Ok(Self {
            inner: RefCell::new(inner),
            output_format,
            meta,
            render_texture_fn_name,
            render_fn_index,
        })
    }

    /// Module metadata (function signatures, uniform/global layouts).
    #[must_use]
    pub fn meta(&self) -> &LpsModuleSig {
        &self.meta
    }

    /// Output format this shader was compiled for.
    #[must_use]
    pub fn output_format(&self) -> TextureStorageFormat {
        self.output_format
    }

    /// Signature of the user `render` function (not the synthesised loop).
    #[must_use]
    pub fn render_sig(&self) -> &LpsFnSig {
        &self.meta.functions[self.render_fn_index]
    }

    /// Render one frame into the given texture buffer.
    ///
    /// `uniforms` should be an [`LpsValueF32::Struct`] whose fields match
    /// `meta().uniforms_type` when the shader declares uniforms.
    pub fn render_frame(
        &self,
        uniforms: &LpsValueF32,
        tex: &mut LpsTextureBuf,
    ) -> Result<(), LpsError> {
        self.apply_uniforms(uniforms)?;

        if tex.format() != self.output_format {
            return Err(LpsError::Render(format!(
                "render_frame: texture format {:?} does not match shader output {:?}",
                tex.format(), self.output_format
            )));
        }

        let w = tex.width();
        let h = tex.height();
        let mut buf = tex.buffer();
        self.inner.borrow_mut().call_render_texture(
            &self.render_texture_fn_name, &mut buf, w, h,
        )
    }

    fn apply_uniforms(&self, uniforms: &LpsValueF32) -> Result<(), LpsError> {
        // (Body unchanged from current px_shader.rs; just swap
        //  `inst.set_uniform(...)` for `self.inner.borrow_mut().set_uniform(...)`.)
        let Some(ref uniforms_type) = self.meta.uniforms_type else { return Ok(()); };
        let LpsType::Struct { members, .. } = uniforms_type else {
            return Err(LpsError::Render(String::from("uniforms_type is not a struct")));
        };
        if members.is_empty() { return Ok(()); }

        let LpsValueF32::Struct { fields, .. } = uniforms else {
            return Err(LpsError::Render(String::from(
                "expected uniforms as LpsValueF32::Struct",
            )));
        };

        let mut inner = self.inner.borrow_mut();
        for member in members {
            let name = member.name.as_deref().ok_or_else(|| {
                LpsError::Render(String::from("uniform member has no name"))
            })?;
            let value = fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
                .ok_or_else(|| LpsError::Render(format!("missing uniform field `{name}`")))?;
            inner.set_uniform(name, value)?;
        }
        Ok(())
    }
}
```

> Trait-object considerations:
> - `Box<dyn PxShaderBackend>` requires `'static` bounds; the
>   `LpvmModule + 'static` bound on the constructor is fine since
>   compiled modules don't borrow from anything.
> - The adapter's `M: LpvmModule + 'static` bound also requires
>   `M::Instance: 'static`. All backends satisfy this today
>   (instances own their state; no borrowed references from the
>   module). If any backend ever changes that, a `BoxedAny`-style
>   workaround would be needed — out of scope.

### `lp-shader/lp-shader/src/engine.rs` — wire the synth into `compile_px`

```rust
pub fn compile_px(
    &self,
    glsl: &str,
    output_format: TextureStorageFormat,
) -> Result<LpsPxShader, LpsError> {           // NEW: monomorphic return type
    let naga = lps_frontend::compile(glsl).map_err(|e| LpsError::Parse(format!("{e}")))?;
    let (mut ir, mut meta) = lps_frontend::lower(&naga)
        .map_err(|e| LpsError::Lower(format!("{e}")))?;
    drop(naga);

    let render_fn_index = validate_render_sig(&meta, output_format)?;

    // NEW: synthesise __render_texture_<format> (Phase 3).
    let render_texture_fn_name = crate::synth::synthesise_render_texture(
        &mut ir, &mut meta, render_fn_index, output_format,
    ).map_err(|e| LpsError::Compile(format!("synth render_texture: {e:?}")))?;

    let module = self.engine.compile(&ir, &meta)
        .map_err(|e| LpsError::Compile(format!("{e}")))?;

    LpsPxShader::new(
        module, meta, output_format, render_fn_index, render_texture_fn_name,
    )
}
```

`validate_render_sig` and `expected_return_type` stay unchanged.

### `lp-shader/lp-shader/src/lib.rs`

Add `pub mod synth;` (Phase 3 created the module; this exposes it
inside the crate). The re-export `pub use px_shader::LpsPxShader;`
remains; the public type is now monomorphic.

### Existing test updates

[`lp-shader/lp-shader/src/tests.rs`](../../../lp-shader/lp-shader/src/tests.rs):

- `render_frame_no_uniforms` / `render_frame_sets_uniforms`:
  the test bodies likely don't reference `LpsPxShader<…>`
  explicitly (they use `compile_px(...)`'s return value directly),
  so they should continue to compile after the generic is dropped.
  If any test file uses `LpsPxShader<E::Module>` in a type
  ascription, drop the generic.
- `render_frame_sets_uniforms` previously verified uniforms were
  applied even though the per-pixel loop was a stub. With the loop
  now real, the test still passes — it doesn't read pixel output.

### Tests added in this phase

Smoke test that the new pipeline compiles end-to-end and produces a
type-erased shader:

```rust
// lp-shader/lp-shader/src/tests.rs (extend)

#[test]
#[cfg(feature = "native")]   // or whichever default backend is wired in tests
fn compile_px_returns_monomorphic_lps_pxshader() {
    let glsl = r#"
        vec4 render(vec2 pos) { return vec4(0.0, 1.0, 0.0, 1.0); }
    "#;
    let engine = lps_engine_for_tests();
    let shader: LpsPxShader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px should succeed for trivial shader");
    assert_eq!(shader.output_format(), TextureStorageFormat::Rgba16Unorm);
    // Synth function must be in meta:
    assert!(shader.meta().functions.iter().any(|f|
        f.name == "__render_texture_rgba16" && f.kind == LpsFnKind::Synthetic
    ));
}
```

(Use the test-helper engine constructor that already exists in
`tests.rs`; if there isn't one, follow the pattern in
`render_frame_sets_uniforms`.)

Actual *pixel-output* correctness lives in Phase 5; this test just
verifies the wiring.

## Validate

```bash
cargo check  -p lp-shader
cargo build  -p lp-shader --features native
cargo build  -p lp-shader --features cranelift
cargo test   -p lp-shader --features native
cargo test   -p lp-shader --features cranelift
```

Both feature builds succeed end-to-end. The pre-existing
`render_frame_sets_uniforms` and `render_frame_no_uniforms` tests
still pass; the new compile-px wiring test passes.

If `lpvm-cranelift`'s Phase 2 JIT smoke test still passes after this
phase, the cross-trait wiring is correct. If it doesn't (because
`compile_px` triggers synth and synth output differs from the
handwritten test fixture), update the smoke test as Phase 5 lands.
