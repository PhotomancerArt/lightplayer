//! Compiled pixel shader: module + instance, uniforms at render time.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::cell::RefCell;

use lps_shared::{LpsFnKind, LpsFnSig, LpsModuleSig, LpsType, LpsValueF32, TextureStorageFormat};
use lpvm::{LpvmBuffer, LpvmInstance, LpvmModule};

use crate::error::LpsError;
use crate::texture_buf::LpsTextureBuf;

/// Backend-erased operations on a compiled pixel shader's runtime instance.
pub(crate) trait PxShaderBackend {
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
/// [`PxShaderBackend`]. Owns both: the module is retained for the lifetime
/// of the instance (compiled code may be referenced by the instance).
struct BackendAdapter<M: LpvmModule> {
    /// Retained so the compiled module outlives `instance` (code may reference module memory).
    _module: M,
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
/// Holds its backend instance behind [`Box<dyn PxShaderBackend>`] so the
/// public type is monomorphic. [`Self::render_frame`] runs the per-pixel loop
/// inside the synthesised `__render_texture_<format>` function via the
/// backend's [`LpvmInstance::call_render_texture`] fast path.
///
/// The instance lives in a [`RefCell`] so [`Self::render_frame`] can
/// take `&self`; mutation goes through runtime borrow checks (panic
/// if re-entrant). [`Send`]/[`Sync`] are implemented only for embedding in
/// the single-threaded engine graph; do not call `render_frame` concurrently.
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
        let synth_sig = meta
            .functions
            .iter()
            .find(|f| f.name == render_texture_fn_name)
            .ok_or_else(|| {
                LpsError::Compile(format!(
                    "compile_px: synthesised function `{render_texture_fn_name}` missing from meta"
                ))
            })?;
        if synth_sig.kind != LpsFnKind::Synthetic {
            return Err(LpsError::Compile(format!(
                "compile_px: function `{render_texture_fn_name}` is not marked Synthetic"
            )));
        }
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

        let instance = module
            .instantiate()
            .map_err(|e| LpsError::Compile(format!("instantiate: {e}")))?;
        let inner: Box<dyn PxShaderBackend> = Box::new(BackendAdapter {
            _module: module,
            instance,
        });

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
    /// Pipeline: when the shader declares uniforms, `uniforms` must be an
    /// [`LpsValueF32::Struct`] whose fields match `meta().uniforms_type`; those
    /// values are applied first. Then the synthesised
    /// `__render_texture_<format>` entry is invoked via
    /// [`LpvmInstance::call_render_texture`], which runs the per-pixel guest
    /// loop and writes packed **unorm16** channel data into the texture buffer
    /// backing `tex`.
    pub fn render_frame(
        &self,
        uniforms: &LpsValueF32,
        tex: &mut LpsTextureBuf,
    ) -> Result<(), LpsError> {
        self.apply_uniforms(uniforms)?;

        if tex.format() != self.output_format {
            return Err(LpsError::Render(format!(
                "render_frame: texture format {:?} does not match shader output {:?}",
                tex.format(),
                self.output_format
            )));
        }

        let w = tex.width();
        let h = tex.height();
        let mut buf = tex.buffer();
        self.inner
            .borrow_mut()
            .call_render_texture(&self.render_texture_fn_name, &mut buf, w, h)
    }

    fn apply_uniforms(&self, uniforms: &LpsValueF32) -> Result<(), LpsError> {
        let Some(ref uniforms_type) = self.meta.uniforms_type else {
            return Ok(());
        };

        let LpsType::Struct { members, .. } = uniforms_type else {
            return Err(LpsError::Render(String::from(
                "uniforms_type is not a struct",
            )));
        };

        if members.is_empty() {
            return Ok(());
        }

        let LpsValueF32::Struct { fields, .. } = uniforms else {
            return Err(LpsError::Render(String::from(
                "expected uniforms as LpsValueF32::Struct",
            )));
        };

        let mut inner = self.inner.borrow_mut();
        for member in members {
            let name = member
                .name
                .as_deref()
                .ok_or_else(|| LpsError::Render(String::from("uniform member has no name")))?;
            let value = fields
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v)
                .ok_or_else(|| LpsError::Render(format!("missing uniform field `{name}`")))?;
            if member.ty == LpsType::Texture2D {
                match value {
                    LpsValueF32::Texture2D(tv) => {
                        let spec = self.meta.texture_specs.get(name).ok_or_else(|| {
                            LpsError::Render(format!(
                                "texture uniform `{name}`: missing texture binding spec in module metadata"
                            ))
                        })?;
                        crate::runtime_texture_validation::validate_runtime_texture_binding(
                            name, tv, spec,
                        )?;
                    }
                    _ => {
                        return Err(LpsError::Render(format!(
                            "texture uniform `{name}` expects `LpsValueF32::Texture2D` (e.g. from `LpsTextureBuf::to_texture2d_value()`)"
                        )));
                    }
                }
            }
            inner.set_uniform(name, value)?;
        }
        Ok(())
    }
}

// SAFETY: Engine invokes `render_frame` from a single thread during rendering.
unsafe impl Send for LpsPxShader {}
unsafe impl Sync for LpsPxShader {}
