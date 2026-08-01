//! [`NullGraphics`]: an [`LpGraphics`] backend that owns no resources and
//! refuses every request.
//!
//! # What this is for, and what it is emphatically not
//!
//! `LpServer` takes `graphics: Arc<dyn LpGraphics>` — not an `Option` — so a
//! host must hand it *some* backend even when it will never run a shader.
//! Constructing the real CPU backend (`lp-gfx-lpvm`) to satisfy that
//! signature links the whole on-device JIT (`lpvm-native`, and the GLSL
//! frontend behind it) into an image that never calls it. `NullGraphics`
//! exists so **constrained bring-up firmware that runs no shaders** can
//! satisfy the signature without paying for the compiler it does not use.
//!
//! **This is never the default, and it is not a step toward making the JIT
//! optional.** Every host, the studio, and the shipping ESP32 firmware
//! construct the real backend; only a deliberately stripped bring-up build
//! opts into this one, at its own construction site. Nothing here
//! feature-gates the compiler out of `lpc-engine` or `lpa-server`, and
//! nothing here makes `LpGraphics` optional — see the hard rule at the top
//! of `AGENTS.md` ("Make the compiler an opt-in feature on `lp-engine` or
//! `lp-server` … STOP. You are about to break the product."). A future
//! reader must not mistake this file for permission to do that.
//!
//! # Behaviour
//!
//! Every trait method fails with [`GfxError::Backend`] — the variant
//! documented as "the backend cannot service the request at all" — carrying
//! a message that names this backend and says the build has no shader
//! support, so a developer who deploys a shader to such a board gets a lead
//! instead of a bare "backend error". It never panics and never returns an
//! empty success (an empty program or a blank texture would make a stripped
//! build look like a *rendering* bug rather than a missing capability).
//!
//! The message is currently the **only** signal a user gets that the build
//! cannot run shaders; the general fix is firmware capability reporting,
//! tracked in `docs/debt/firmware-capability-reporting.md`.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use lps_shared::TextureStorageFormat;

use crate::compute_shader::LpComputeShader;
use crate::gfx_error::GfxError;
use crate::graphics::LpGraphics;
use crate::sample_out_handle::SampleOutHandle;
use crate::sample_points_handle::SamplePointsHandle;
use crate::shader::LpShader;
use crate::shader_compile_options::ShaderCompileOptions;
use crate::shader_semantics::ShaderSemantics;
use crate::texture_data::TextureData;
use crate::texture_handle::TextureHandle;

/// Label this backend reports to logs and error text.
const NULL_BACKEND_NAME: &str = "null-graphics";

/// An [`LpGraphics`] that compiles nothing, allocates nothing, and fails
/// every request with a self-describing [`GfxError::Backend`].
///
/// For bring-up firmware that runs no shaders and would otherwise link the
/// on-device compiler purely to satisfy `LpServer`'s
/// `Arc<dyn LpGraphics>` parameter. See the module docs — in particular,
/// this is never a default and never a reason to gate the compiler out of
/// the engine or server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NullGraphics;

impl NullGraphics {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl LpGraphics for NullGraphics {
    fn compile_shader(
        &self,
        _source: &str,
        _options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, GfxError> {
        Err(unsupported("compile a GLSL shader"))
    }

    fn compile_compute_shader(
        &self,
        _desc: lp_shader::CompileComputeDesc<'_>,
    ) -> Result<Box<dyn LpComputeShader>, GfxError> {
        // Overrides the trait default, which says only "graphics backend
        // does not support compute shaders" — true of accelerated backends
        // too, and it would not tell a confused developer that *this build*
        // has no shader support at all.
        Err(unsupported("compile a compute shader"))
    }

    fn backend_name(&self) -> &'static str {
        NULL_BACKEND_NAME
    }

    // `native_semantics` and `glsl_frontend` are the two per-backend product
    // decisions render paths read *before* compiling. A backend that never
    // compiles has no meaningful choice to state, and both answers here are
    // unreachable-by-construction: every path that consults them feeds
    // `compile_shader`, which always fails first. Both are stated anyway,
    // and stated as fixed constants rather than `cfg!`/feature expressions,
    // because the contract those two methods exist to enforce is "the value
    // does not vary with the build graph" (Cargo feature unification must
    // not change compile behaviour). A constant honours that contract; a
    // panic or a `cfg!` would not.
    fn native_semantics(&self) -> ShaderSemantics {
        // Q32 is the on-device product tier, so this backend does not look
        // like a preview/GPU tier in any log or UI that reads it.
        ShaderSemantics::Q32
    }

    fn glsl_frontend(&self) -> lp_shader::ShaderFrontend {
        // `LpsGlsl` rather than `Naga` specifically because
        // `ShaderFrontend::built_in()` is unconditionally true for it,
        // whereas `Naga` is true only under the `naga` feature. A host that
        // const-asserts `backend.glsl_frontend().built_in()` must not trip
        // over this placeholder as a side effect of the feature graph.
        lp_shader::ShaderFrontend::LpsGlsl
    }

    fn create_render_target(&self, _width: u32, _height: u32) -> Result<TextureHandle, GfxError> {
        Err(unsupported("allocate a render target"))
    }

    fn create_texture(
        &self,
        _width: u32,
        _height: u32,
        _format: TextureStorageFormat,
        _texels: &[u8],
    ) -> Result<TextureHandle, GfxError> {
        Err(unsupported("allocate a texture"))
    }

    fn write_texture(&self, _texture: &mut TextureHandle, _texels: &[u8]) -> Result<(), GfxError> {
        Err(unsupported("write texels into a texture"))
    }

    fn clear_texture(&self, _texture: &mut TextureHandle) -> Result<(), GfxError> {
        Err(unsupported("clear a texture"))
    }

    fn blend_textures(
        &self,
        _previous: &TextureHandle,
        _active: &TextureHandle,
        _alpha: f32,
        _target: &mut TextureHandle,
    ) -> Result<(), GfxError> {
        Err(unsupported("blend textures"))
    }

    fn read_back(&self, _texture: &TextureHandle) -> Result<TextureData, GfxError> {
        Err(unsupported("read a texture back"))
    }

    fn supports_read_back(&self) -> bool {
        // Keeps the trait default (`true`), stated explicitly because the
        // reasoning is not obvious: this backend can read nothing back, but
        // it can also never hand out a handle to read. Answering `false`
        // would route callers down the GPU-resident path — a *second*,
        // misleading behaviour on a backend whose whole job is to fail once,
        // clearly, at the allocation call.
        true
    }

    fn create_sample_points(&self, _count: u32) -> Result<SamplePointsHandle, GfxError> {
        Err(unsupported("allocate a sample-point buffer"))
    }

    fn write_sample_points(
        &self,
        _points: &mut SamplePointsHandle,
        _xy_q16: &[i32],
    ) -> Result<(), GfxError> {
        Err(unsupported("write sample points"))
    }

    fn read_sample_points(&self, _points: &SamplePointsHandle) -> Result<Vec<i32>, GfxError> {
        Err(unsupported("read sample points back"))
    }

    fn create_sample_out(&self, _count: u32) -> Result<SampleOutHandle, GfxError> {
        Err(unsupported("allocate a sample-output buffer"))
    }

    fn write_sample_out(
        &self,
        _out: &mut SampleOutHandle,
        _rgba16: &[u16],
    ) -> Result<(), GfxError> {
        Err(unsupported("write sample outputs"))
    }

    fn read_sample_out(&self, _out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
        Err(unsupported("read sample outputs back"))
    }

    fn clear_sample_out(&self, _out: &mut SampleOutHandle) -> Result<(), GfxError> {
        Err(unsupported("clear a sample-output buffer"))
    }
}

/// The one failure every [`NullGraphics`] method returns.
///
/// `request` completes "…so it cannot {request}". Engine-side wrappers
/// prepend their own context (e.g. `"create_render_target: {error}"`), so
/// this text stays a standalone explanation rather than a fragment.
fn unsupported(request: &str) -> GfxError {
    GfxError::Backend(format!(
        "{NULL_BACKEND_NAME}: this build has no shader support, so it cannot {request} — \
         it runs the null graphics backend in place of a real one (lp-gfx-lpvm). \
         Rebuild with shader support to run shaders."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::string::ToString;
    use alloc::sync::Arc;

    #[test]
    fn compile_shader_errors_rather_than_panicking_or_returning_a_program() {
        let graphics = NullGraphics::new();
        let options =
            ShaderCompileOptions::new(ShaderSemantics::Q32, lp_shader::ShaderFrontend::LpsGlsl);

        let error = graphics
            .compile_shader("void main() {}", &options)
            .err()
            .expect("null graphics must refuse to compile");

        assert!(
            matches!(error, GfxError::Backend(_)),
            "expected GfxError::Backend, got {error:?}"
        );
    }

    #[test]
    fn error_text_names_the_backend_and_the_missing_capability() {
        let message = unsupported("compile a GLSL shader").to_string();

        assert!(message.contains(NULL_BACKEND_NAME), "{message}");
        assert!(message.contains("no shader support"), "{message}");
        assert!(message.contains("compile a GLSL shader"), "{message}");
        // The lead a confused developer follows to a fix.
        assert!(message.contains("lp-gfx-lpvm"), "{message}");
    }

    #[test]
    fn every_allocating_method_errors() {
        let graphics = NullGraphics::new();

        assert!(graphics.create_render_target(8, 8).is_err());
        assert!(
            graphics
                .create_texture(1, 1, TextureStorageFormat::Rgba16Unorm, &[0; 8])
                .is_err()
        );
        assert!(graphics.create_sample_points(4).is_err());
        assert!(graphics.create_sample_out(4).is_err());
    }

    #[test]
    fn handle_taking_methods_error_rather_than_panicking() {
        // Handles can only come from a backend that allocates, so pin the
        // no-panic property with handles from a stand-in allocator: a
        // foreign handle must produce the same clean failure.
        let graphics = NullGraphics::new();
        let allocator: Arc<dyn crate::HandleAllocator> = Arc::new(NoopAllocator);

        let mut texture = TextureHandle::from_backend_parts(
            1,
            1,
            TextureStorageFormat::Rgba16Unorm,
            Box::new(()),
            Arc::clone(&allocator),
        );
        let mut points =
            SamplePointsHandle::from_backend_parts(1, Box::new(()), Arc::clone(&allocator));
        let mut out = SampleOutHandle::from_backend_parts(1, Box::new(()), Arc::clone(&allocator));

        assert!(graphics.write_texture(&mut texture, &[0; 8]).is_err());
        assert!(graphics.clear_texture(&mut texture).is_err());
        assert!(graphics.read_back(&texture).is_err());
        assert!(graphics.write_sample_points(&mut points, &[0, 0]).is_err());
        assert!(graphics.read_sample_points(&points).is_err());
        assert!(graphics.write_sample_out(&mut out, &[0; 4]).is_err());
        assert!(graphics.read_sample_out(&out).is_err());
        assert!(graphics.clear_sample_out(&mut out).is_err());

        let other = TextureHandle::from_backend_parts(
            1,
            1,
            TextureStorageFormat::Rgba16Unorm,
            Box::new(()),
            allocator,
        );
        assert!(
            graphics
                .blend_textures(&other, &other, 0.5, &mut texture)
                .is_err()
        );
    }

    #[test]
    fn states_a_fixed_frontend_and_tier_that_no_feature_can_move() {
        let graphics = NullGraphics::new();

        assert_eq!(graphics.backend_name(), NULL_BACKEND_NAME);
        assert_eq!(graphics.native_semantics(), ShaderSemantics::Q32);
        assert_eq!(graphics.glsl_frontend(), lp_shader::ShaderFrontend::LpsGlsl);
        // The reason `LpsGlsl` is the stated placeholder: it is built in
        // regardless of the `naga` feature, so a host that const-asserts
        // `built_in()` on the backend's answer never trips over it.
        assert!(graphics.glsl_frontend().built_in());
        assert!(graphics.supports_read_back());
    }

    #[test]
    fn is_usable_as_the_injected_trait_object() {
        // `LpServer` takes `Arc<dyn LpGraphics>`; the whole point is that
        // this satisfies that parameter.
        let graphics: Arc<dyn LpGraphics> = Arc::new(NullGraphics::new());
        assert_eq!(graphics.backend_name(), NULL_BACKEND_NAME);
    }

    /// Stand-in for a real backend's deallocation vtable, so tests can build
    /// the opaque handles the handle-taking methods accept.
    struct NoopAllocator;

    impl crate::HandleAllocator for NoopAllocator {
        fn free_texture(&self, _backing: crate::HandleBacking) {}
        fn free_sample_points(&self, _backing: crate::HandleBacking) {}
        fn free_sample_out(&self, _backing: crate::HandleBacking) {}
    }
}
