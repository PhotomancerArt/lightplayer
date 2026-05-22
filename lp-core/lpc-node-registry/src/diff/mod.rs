//! Project snapshot diff and equivalence checks (ChangeSet M6).

mod def_diff;
mod equivalence;
mod project_diff;
mod snapshot;

pub use equivalence::{DiffError, assert_equivalent};
pub use project_diff::diff;
pub use snapshot::ProjectSnapshot;
