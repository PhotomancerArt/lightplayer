//! The space a shader declares it renders in, and the GLSL entry that
//! follows from it.
//!
//! Entries are **explicit and declaration-driven** (dimensionality plan
//! D19): a shader declares `TwoD` or `OneD` and must define exactly the
//! matching entry —
//!
//! | declaration | entry |
//! |---|---|
//! | [`ShaderEntrySpace::TwoD`] | `vec4 render_2d(vec2 pos)` |
//! | [`ShaderEntrySpace::OneD`] | `vec4 render_1d(float pos)` |
//!
//! There is no `render(vec2)` any more and no alias for it: a function
//! named `render` is a hard compile error carrying migration guidance
//! ([`crate::LpsError::Validation`]). The declaration is a per-compile
//! choice carried on [`crate::CompilePxDesc::space`], exactly like
//! `float_mode` — the compiler never infers the space from the source,
//! because "which entry did you mean" is an authoring decision and a
//! silently-inferred answer is the failure this contract removes.
//!
//! `outputSize` stays a `vec2` in both spaces; a 1D target reports
//! `(N, 1)`.

/// The declared space of a pixel shader: which entry it defines and how
/// coordinates reach it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum ShaderEntrySpace {
    /// Renders into 2D texture space through `vec4 render_2d(vec2 pos)`.
    /// The default: every shader authored before the dimensionality plan
    /// is 2D.
    #[default]
    TwoD,
    /// Renders along a 1D strip through `vec4 render_1d(float pos)`.
    OneD,
}

impl ShaderEntrySpace {
    /// The GLSL entry name this declaration requires.
    #[must_use]
    pub const fn entry_name(self) -> &'static str {
        match self {
            Self::TwoD => "render_2d",
            Self::OneD => "render_1d",
        }
    }

    /// The entry name of the *other* space — what a cross-validation error
    /// found instead of [`Self::entry_name`].
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::TwoD => Self::OneD,
            Self::OneD => Self::TwoD,
        }
    }

    /// Short human label used in diagnostics ("2D" / "1D").
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TwoD => "2D",
            Self::OneD => "1D",
        }
    }

    /// The entry's full authored signature, quoted in diagnostics.
    #[must_use]
    pub const fn entry_signature(self) -> &'static str {
        match self {
            Self::TwoD => "vec4 render_2d(vec2 pos)",
            Self::OneD => "vec4 render_1d(float pos)",
        }
    }

    /// Number of coordinate lanes the entry takes: 2 for `render_2d`, 1 for
    /// `render_1d`. This is also the packed lane count of a sample-point
    /// batch (see [`crate::synth::render_samples`]).
    #[must_use]
    pub const fn coord_lanes(self) -> usize {
        match self {
            Self::TwoD => 2,
            Self::OneD => 1,
        }
    }
}

/// The entry a compile resolved to: the declared space plus the index of
/// its signature in [`lps_shared::LpsModuleSig::functions`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderEntry {
    pub space: ShaderEntrySpace,
    pub index: usize,
}
