//! Format 11, frozen: where every lamp of a format-11 project lands,
//! computed from its files alone (the v11→v12 step's arithmetic), and that
//! landing written down as map2d format 6 (the step's freeze).

mod freeze_document;
mod freeze_fit;
mod freeze_ids;
mod freeze_patching;
mod v11_files;
mod v11_filled_polygon_cells;
mod v11_landing;
mod v11_patch;
mod v11_project;
mod v11_project_landing;
mod v11_segments;

pub use v11_landing::landing;
pub use v11_project_landing::{
    DarkReason, FixtureLanding, FixtureLayout, LandedPiece, Landing, OutputLanding, ProjectLanding,
    SegmentLanding,
};
pub use v11_segments::SegmentSource;

pub(crate) use freeze_document::{
    map2d6_document, object_ids_and_names, objects_to_name, points_document, strided_objects,
};
pub(crate) use freeze_fit::{ExactFit, exact_fit};
pub(crate) use freeze_ids::{PartIds, mint_part_ids};
pub(crate) use freeze_patching::{FrozenPatching, OutputPorts, freeze_patching};
pub(crate) use v11_files::{read_json, refused, scalar};
pub(crate) use v11_project::{V11Mapping, V11Project};
