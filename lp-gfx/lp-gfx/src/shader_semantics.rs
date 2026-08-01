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
    /// IEEE f32 on a CPU tier — native `f32` executed by the LPVM backend,
    /// normatively specified in `docs/design/float.md`.
    ///
    /// Distinct from [`Self::F32Gpu`] rather than folded into one "float"
    /// variant because the two have different fidelity contracts: the GPU tier
    /// carries documented divergence latitude
    /// (`docs/adr/2026-07-09-preview-fidelity-tiers.md`), while this tier is
    /// held to `float.md` exactly. A backend accepts the tiers it implements
    /// and rejects the rest — a shader must never be told it got one and run
    /// as the other.
    ///
    /// What a request in this tier actually became — hardware FPU or
    /// soft-float calls — is *reported* by
    /// [`crate::ShaderCompileStats::float_impl`], not requested
    /// (`docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md`,
    /// decision 2).
    F32Cpu,
}
