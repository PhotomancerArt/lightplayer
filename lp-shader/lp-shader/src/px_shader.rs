//! Compiled pixel shader: module + instance, uniforms at render time.

use alloc::format;
use alloc::string::String;
use core::cell::RefCell;

use lps_shared::{LpsFnSig, LpsModuleSig, LpsType, LpsValueF32, TextureStorageFormat};
use lpvm::LpvmInstance;
use lpvm::LpvmModule;

use crate::error::LpsError;
use crate::texture_buf::LpsTextureBuf;

/// A compiled pixel shader with internal execution state.
///
/// Combines module + instance internally. Uniforms are passed into [`Self::render_frame`],
/// not stored as separate mutable state on this type (aside from the internal instance).
///
/// The instance lives in a [`RefCell`] so [`Self::render_frame`] can take `&self`: mutation
/// goes through runtime borrow checks (panic if re-entrant). This type is `!Sync` as a result.
pub struct LpsPxShader<M: LpvmModule> {
    // Instance may depend on code owned by the module; keep both in one struct.
    #[allow(dead_code, reason = "retain compiled module for instance lifetime")]
    module: M,
    instance: RefCell<M::Instance>,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
    /// Index of the `render` function in `meta.functions`.
    render_fn_index: usize,
}

impl<M: LpvmModule> LpsPxShader<M> {
    pub(crate) fn new(
        module: M,
        meta: LpsModuleSig,
        output_format: TextureStorageFormat,
        render_fn_index: usize,
    ) -> Result<Self, LpsError> {
        let instance = module
            .instantiate()
            .map_err(|e| LpsError::Compile(format!("instantiate: {e}")))?;
        Ok(Self {
            module,
            instance: RefCell::new(instance),
            output_format,
            meta,
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

    /// Signature of the `render` function.
    #[must_use]
    pub fn render_sig(&self) -> &LpsFnSig {
        &self.meta.functions[self.render_fn_index]
    }

    /// Render one frame into the given texture buffer.
    ///
    /// `uniforms` should be an [`LpsValueF32::Struct`] whose fields match
    /// `meta().uniforms_type` when the shader declares uniforms.
    ///
    /// # M0 / roadmap M2
    ///
    /// This sets uniforms on the internal instance. The per-pixel loop and
    /// writing pixels to `tex` are deferred to roadmap M2.
    pub fn render_frame(
        &self,
        uniforms: &LpsValueF32,
        _tex: &mut LpsTextureBuf,
    ) -> Result<(), LpsError> {
        self.apply_uniforms(uniforms)?;
        Ok(())
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

        let mut inst = self.instance.borrow_mut();
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
            inst
                .set_uniform(name, value)
                .map_err(|e| LpsError::Render(format!("set uniform `{name}`: {e}")))?;
        }
        Ok(())
    }
}
