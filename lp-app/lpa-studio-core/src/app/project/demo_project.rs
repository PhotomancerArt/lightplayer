use lpa_client::ProjectDeployFile;

use crate::STUDIO_DEMO_PROJECT_ID;
use crate::app::home::embedded_example::{ExampleFile, embedded_example};

pub const DEMO_PROJECT_ID: &str = STUDIO_DEMO_PROJECT_ID;
pub const DEMO_PROJECT_STORAGE_ID: &str = "studio";

/// The Studio demo project — `catalog/fyeah-sign`.
///
/// Chosen over the minimal `catalog/plasma` so the demo exercises the full
/// bus: a clock (time), a button + radio bridge (both writing `bus:trigger`),
/// and a playlist switching between idle and blast visuals. The button/radio
/// are virtual in the browser sim, so nothing physically fires, but every
/// binding registers — the module card's wiring drawer shows the real
/// topology.
///
/// The file list itself is the catalog registry's entry for
/// [`STUDIO_DEMO_PROJECT_ID`]: the demo the sim boots and the example the
/// gallery opens are the same bytes by construction, not by two lists
/// agreeing.
pub fn demo_project_files() -> &'static [ExampleFile] {
    embedded_example(DEMO_PROJECT_ID)
        .unwrap_or_else(|| panic!("the demo project {DEMO_PROJECT_ID} is in the catalog"))
        .files
}

pub fn demo_project_deploy_files() -> Vec<ProjectDeployFile> {
    demo_project_files()
        .iter()
        .map(|(relative_path, bytes)| ProjectDeployFile::new(*relative_path, bytes.to_vec()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_project_identity_uses_fyeah_sign() {
        assert_eq!(DEMO_PROJECT_ID, "catalog/fyeah-sign");
        assert_eq!(DEMO_PROJECT_STORAGE_ID, "studio");
    }

    #[test]
    fn demo_project_files_are_the_fyeah_sign_example() {
        let files = demo_project_files();

        assert!(
            files.iter().any(|(path, _)| *path == "playlist.json"),
            "fyeah-sign demo must include the playlist node"
        );
        // The same bytes the checked-in entry holds, by construction: the
        // registry is generated from the tree, so the demo cannot drift
        // from `catalog/projects/fyeah-sign/`.
        let checked_in = |name: &str| {
            std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../catalog/projects/fyeah-sign")
                    .join(name),
            )
            .expect("read the checked-in file")
        };
        for name in ["project.json", "module.json"] {
            assert_eq!(
                files.iter().find(|(path, _)| *path == name).unwrap().1,
                checked_in(name).as_slice(),
                "{name} must be the checked-in bytes"
            );
        }
        // The fixture's mapping document must deploy with the project — its
        // absence fails the fixture at load (found the hard way when the M2
        // migration updated fixture.json but not this compiled-in list).
        assert!(
            files.iter().any(|(path, _)| *path == "fyeah.map2d.json"),
            "fyeah-sign demo must include the mapping document"
        );
    }
}
