//! The fixture's rule for samples past a bounded product's extent.

use crate::space::address_mode::AddressMode;

/// What a sample that lands past a bounded product's extent becomes (theory
/// D6, D14, D18): per axis, an address mode; for [`AddressMode::Solid`], the
/// colour. The fixture authors it beside the fit; products do not suggest
/// one (D13).
///
/// This is the *product's* outside. A lamp outside the *fixture's* box is
/// dark whatever this says (D14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutsideRule {
    pub x: AddressMode,
    pub y: AddressMode,
    /// The colour a [`AddressMode::Solid`] axis gives, sRGB.
    pub solid: [u8; 3],
}

/// The default outside rule: hold the edge on both axes, today's silent
/// behaviour, so a newly bounded source never shows a black frame by
/// surprise (open question O2's lean, pending Yona at G1 — change this one
/// line if he answers solid).
pub const DEFAULT_OUTSIDE_RULE: OutsideRule = OutsideRule::uniform(AddressMode::Clamp);

impl OutsideRule {
    /// The same mode on both axes, with a black solid colour.
    #[must_use]
    pub const fn uniform(mode: AddressMode) -> Self {
        Self {
            x: mode,
            y: mode,
            solid: [0, 0, 0],
        }
    }
}

impl Default for OutsideRule {
    fn default() -> Self {
        DEFAULT_OUTSIDE_RULE
    }
}
