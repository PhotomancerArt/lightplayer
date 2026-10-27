//! A format-6 fixture's mapping, as the engine holds it: the model's
//! [`Mapping`] (parts and patching) plus the two layout facts the fit into
//! the fixture's texture needs, the canvas and the sample diameter.
//!
//! The in-memory model carries neither yet (notes C13): until M1 P3 decides
//! where they live in the format (lean Q51: on `Mapping`), they ride beside
//! it here, and M2 P4's loader fills them from wherever P3 put them.

use lpc_mapping::Bounds2d;
use lpc_mapping::part::Mapping;

/// A fixture's parts mapping and its fit facts.
#[derive(Clone, Debug, PartialEq)]
pub struct FixturePartsSource {
    /// Its parts and its patching (starts, flows, overrides).
    pub mapping: Mapping,
    /// The drawing's frame `[min_x, min_y, width, height]`, as a map2d
    /// document's `canvas`: what the lamps are fitted into the texture by.
    /// `None` fits the lamps' own bounds.
    pub canvas: Option<[f32; 4]>,
    /// The sampling diameter, in the drawing's units (fitted with the
    /// lamps).
    pub sample_diameter: f32,
}

impl FixturePartsSource {
    /// The canvas as the fit reads it; a canvas of no width or height is
    /// none (as a map2d document's is).
    #[must_use]
    pub fn canvas_bounds(&self) -> Option<Bounds2d> {
        let [min_x, min_y, width, height] = self.canvas?;
        (width > f32::EPSILON && height > f32::EPSILON).then_some(Bounds2d {
            min_x,
            min_y,
            width,
            height,
        })
    }
}
