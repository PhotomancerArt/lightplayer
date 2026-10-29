//! Why a space could not be built.

/// A box, fit or transform that has no meaning, refused when it is built so
/// nothing downstream divides by zero or carries a NaN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceError {
    /// A fixture box with a width or height that is not positive and
    /// finite, or a corner that is not finite.
    EmptyViewBox,
    /// A product box with a width or height that is not positive and
    /// finite (a `shadertoy` view of 0 pixels, an extent of no size).
    EmptyProductBox,
    /// A fit scale that is not positive and finite.
    BadScale,
    /// A rotation that is not finite.
    BadRotation,
}

impl core::fmt::Display for SpaceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::EmptyViewBox => "the fixture's box needs a positive width and height",
            Self::EmptyProductBox => "the source's box needs a positive width and height",
            Self::BadScale => "the fit's scale needs to be a positive number",
            Self::BadRotation => "the fit's rotation needs to be a number",
        })
    }
}
