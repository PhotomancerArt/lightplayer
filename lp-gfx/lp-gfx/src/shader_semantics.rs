//! Explicit numeric-semantics tier for shader compilation.

/// Numeric semantics a shader must be compiled with.
///
/// Per `docs/adr/2026-07-09-preview-fidelity-tiers.md`, the tier is explicit
/// caller state: a backend that cannot honor the requested tier must fail
/// compilation with [`crate::GfxError::Backend`] — silently substituting
/// different semantics (e.g. ignoring Q32 options on a float GPU) is never
/// allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShaderSemantics {
    /// Authoritative Q16.16 fixed-point semantics — the on-device product
    /// tier, honoring [`crate::ShaderCompileOptions::q32_options`].
    #[default]
    Q32,
    /// IEEE f32 GPU semantics — the preview/non-embedded tier. Q32 options do
    /// not apply; conformance is judged against the f32 interpreter oracle.
    F32Gpu,
}
