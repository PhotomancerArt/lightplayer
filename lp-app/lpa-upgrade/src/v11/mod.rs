//! Format 11, frozen: where every lamp of a format-11 project lands,
//! computed from its files alone (the v11→v12 step's arithmetic).

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
