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
    /// tier. Arithmetic wraps on overflow and divides by reciprocal
    /// multiplication (`docs/design/q32.md`).
    #[default]
    Q32,
    /// IEEE f32 GPU semantics — the preview/non-embedded tier. Conformance is
    /// judged against the f32 interpreter oracle.
    F32Gpu,
}
