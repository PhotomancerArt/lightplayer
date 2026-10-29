use crate::{AssetSlot, LpPathBuf, Slotted};

/// Fixture-to-texture mapping authored on a fixture definition.
///
/// Authored mappings are `Map2d` documents (`*.map2d.json`, map2d format 6
/// since project format 12): the document holds the parts, their lamps and
/// their patching. Hand-placed lamps are a document's `points` parts; the
/// slot-modelled `PathPoints` lamps were retired with project format 12 (its
/// upgrade turned them into `points` parts), as the SVG imports and
/// parametric ring arrays were before them.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum MappingConfig {
    /// No authored fixture mapping has been selected yet.
    #[default]
    Unset,

    /// An opaque 2D mapping document (`*.map2d.json`) resolved at project
    /// load. The document's contents (objects, sample diameter, canvas) are
    /// deliberately not slot-modeled; `lpc-mapping` owns the schema.
    Map2d { source: AssetSlot },
}

impl MappingConfig {
    pub fn map2d(source: impl Into<LpPathBuf>) -> Self {
        Self::Map2d {
            source: AssetSlot::path(source),
        }
    }
}
