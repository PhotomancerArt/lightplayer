#![allow(
    dead_code,
    unused_imports,
    reason = "shared test support: each test binary only uses a subset"
)]

pub mod assertions;
pub mod identifiers;
pub mod project_files;
pub mod scenario;
pub mod test_project;

pub use assertions::{assert_artifact_asset_content_types, assert_loaded_def_kinds};
pub use identifiers::{artifact, artifact_asset, root_def};
pub use scenario::{RegistryScenario, playlist_use};
pub use test_project::TestProject;
