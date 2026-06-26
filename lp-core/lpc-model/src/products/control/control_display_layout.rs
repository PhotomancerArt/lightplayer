//! Optional human-facing control product display metadata.
//!
//! Display layout is distinct from sample layout. Sample layout describes the
//! native output buffer; display layout describes where logical lamps should be
//! drawn in a UI when a producer can provide that information.

use alloc::vec::Vec;

use crate::project::Revision;

/// Optional control-product geometry for user-facing previews.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ControlDisplayLayout {
    /// A normalized two-dimensional lamp layout.
    Layout2d(ControlLayout2d),
}

impl ControlDisplayLayout {
    #[must_use]
    pub const fn revision(&self) -> Revision {
        match self {
            Self::Layout2d(layout) => layout.revision,
        }
    }
}

/// Normalized two-dimensional lamp display layout.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlLayout2d {
    pub revision: Revision,
    pub width_hint: u32,
    pub height_hint: u32,
    pub lamps: Vec<ControlLamp2d>,
}

impl ControlLayout2d {
    #[must_use]
    pub const fn new(
        revision: Revision,
        width_hint: u32,
        height_hint: u32,
        lamps: Vec<ControlLamp2d>,
    ) -> Self {
        Self {
            revision,
            width_hint,
            height_hint,
            lamps,
        }
    }
}

/// One logical lamp in a two-dimensional display layout.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlLamp2d {
    pub lamp_index: u32,
    pub sample_start: u32,
    pub center: [f32; 2],
    pub radius: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_layout_exposes_revision() {
        let revision = Revision::new(9);
        let layout =
            ControlDisplayLayout::Layout2d(ControlLayout2d::new(revision, 16, 9, Vec::new()));

        assert_eq!(layout.revision(), revision);
    }
}
