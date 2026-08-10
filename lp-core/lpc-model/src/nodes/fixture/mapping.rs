use alloc::vec::Vec;
use lp_collection::VecMap;

use crate::{
    AssetSlot, EnumSlot, LpPathBuf, MapSlot, PositiveF32, PositiveF32Slot, Slotted, ValueSlot, Xy,
    XySlot,
};

/// Fixture-to-texture mapping authored on a fixture definition.
///
/// Authored mappings are `Map2d` documents (`*.map2d.json`); `PathPoints`
/// with `PointList` paths is the RESOLVED runtime carrier those documents
/// funnel into (and remains directly authorable for hand-placed lamps).
/// The legacy authored variants — SVG imports and parametric ring arrays —
/// were retired in the 2D mapping system work; the map2d document owns
/// parametric shapes now.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum MappingConfig {
    /// No authored fixture mapping has been selected yet.
    #[default]
    Unset,

    /// A mapping defined by fixture paths sampled from the target texture.
    PathPoints {
        paths: MapSlot<u32, EnumSlot<PathSpec>>,
        sample_diameter: PositiveF32Slot,
    },

    /// An opaque 2D mapping document (`*.map2d.json`) resolved at project
    /// load. The document's contents (objects, sample diameter, canvas) are
    /// deliberately not slot-modeled; `lpc-mapping` owns the schema.
    Map2d { source: AssetSlot },
}

/// Where a fixture's lamps land on its output's wire, authored on a fixture
/// definition.
///
/// Deliberately shaped like [`MappingConfig`]'s `Map2d` arm and deliberately
/// a SEPARATE slot: mapping is built once and patching changes on every
/// install, so an installer clears the patch (back to `Unset`, pure
/// auto-flow) without disturbing a single lamp position. `lpc-mapping` owns
/// the document schema; nothing here is slot-modeled.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum PatchConfig {
    /// No patch: this fixture's lamps auto-flow onto the wire, in fixture
    /// order, after everything anchored ahead of them.
    #[default]
    Unset,

    /// An opaque patch document (`*.patch.json`) resolved at project load.
    File { source: AssetSlot },
}

impl PatchConfig {
    pub fn file(source: impl Into<LpPathBuf>) -> Self {
        Self::File {
            source: AssetSlot::path(source),
        }
    }

    /// The patch document this fixture references, if it has one.
    pub fn source(&self) -> Option<&AssetSlot> {
        match self {
            Self::Unset => None,
            Self::File { source } => Some(source),
        }
    }
}

/// Specifies one path for a fixture.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum PathSpec {
    /// Explicit lamp positions sampled from an authored mapping source.
    #[default]
    PointList {
        first_channel: ValueSlot<u32>,
        points: MapSlot<u32, XySlot>,
    },
}

impl MappingConfig {
    pub fn path_points(paths: MapSlot<u32, EnumSlot<PathSpec>>, sample_diameter: f32) -> Self {
        Self::PathPoints {
            paths,
            sample_diameter: PositiveF32Slot::new(PositiveF32(sample_diameter)),
        }
    }

    pub fn path_points_vec(paths: Vec<PathSpec>, sample_diameter: f32) -> Self {
        let mut entries = VecMap::new();
        for (index, path) in paths.into_iter().enumerate() {
            entries.insert(index as u32, EnumSlot::new(path));
        }
        Self::path_points(MapSlot::new(entries), sample_diameter)
    }

    pub fn map2d(source: impl Into<LpPathBuf>) -> Self {
        Self::Map2d {
            source: AssetSlot::path(source),
        }
    }
}

impl PathSpec {
    pub fn point_list(first_channel: u32, points: impl IntoIterator<Item = [f32; 2]>) -> Self {
        let mut entries = VecMap::new();
        for (index, point) in points.into_iter().enumerate() {
            entries.insert(index as u32, XySlot::new(Xy(point)));
        }
        Self::PointList {
            first_channel: ValueSlot::new(first_channel),
            points: MapSlot::new(entries),
        }
    }
}
