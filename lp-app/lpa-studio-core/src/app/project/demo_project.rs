use lpa_client::ProjectDeployFile;

use crate::STUDIO_DEMO_PROJECT_ID;
use crate::app::home::embedded_example::{ExampleFile, FYEAH_SIGN_FILES};

pub const DEMO_PROJECT_ID: &str = STUDIO_DEMO_PROJECT_ID;
pub const DEMO_PROJECT_STORAGE_ID: &str = "studio";

/// The Studio demo project — `examples/fyeah-sign`.
///
/// Chosen over the minimal `examples/basic` so the demo exercises the full
/// bus: a clock (time), a button + radio bridge (both writing `bus:trigger`),
/// and a playlist switching between idle and blast visuals. The button/radio
/// are virtual in the browser sim, so nothing physically fires, but every
/// binding registers — the module card's wiring drawer shows the real
/// topology.
///
/// The file list itself is the gallery's
/// [`crate::app::home::embedded_example::FYEAH_SIGN_FILES`] table: the demo
/// the sim boots and the example the gallery opens are the same bytes by
/// construction, not by two lists agreeing.
pub fn demo_project_files() -> &'static [ExampleFile] {
    FYEAH_SIGN_FILES
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
        assert_eq!(DEMO_PROJECT_ID, "examples/fyeah-sign");
        assert_eq!(DEMO_PROJECT_STORAGE_ID, "studio");
    }

    #[test]
    fn demo_project_files_are_the_fyeah_sign_example() {
        let files = demo_project_files();

        assert!(
            files.iter().any(|(path, _)| *path == "playlist.json"),
            "fyeah-sign demo must include the playlist node"
        );
        assert_eq!(
            files
                .iter()
                .find(|(path, _)| *path == "project.json")
                .unwrap()
                .1,
            include_bytes!("../../../../../examples/fyeah-sign/project.json")
        );
        assert_eq!(
            files
                .iter()
                .find(|(path, _)| *path == "module.json")
                .unwrap()
                .1,
            include_bytes!("../../../../../examples/fyeah-sign/module.json")
        );
        // The fixture's mapping document must deploy with the project — its
        // absence fails the fixture at load (found the hard way when the M2
        // migration updated fixture.json but not this compiled-in list).
        assert!(
            files.iter().any(|(path, _)| *path == "fyeah.map2d.json"),
            "fyeah-sign demo must include the mapping document"
        );
    }
}
