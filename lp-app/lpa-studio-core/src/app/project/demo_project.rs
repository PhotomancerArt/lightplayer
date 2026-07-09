use lpa_client::ProjectDeployFile;

use crate::STUDIO_DEMO_PROJECT_ID;

pub const DEMO_PROJECT_ID: &str = STUDIO_DEMO_PROJECT_ID;
pub const DEMO_PROJECT_STORAGE_ID: &str = "studio";

pub struct DemoProjectFile {
    pub relative_path: &'static str,
    pub bytes: &'static [u8],
}

/// The Studio demo project — `examples/fyeah-sign`.
///
/// Chosen over the minimal `examples/basic` so the demo exercises the full
/// bus: a clock (time), a button + radio bridge (both writing `bus:trigger`),
/// and a playlist switching between idle and blast visuals. The button/radio
/// are virtual in the browser sim, so nothing physically fires, but every
/// binding registers — the bus pane shows the real topology.
pub fn demo_project_files() -> &'static [DemoProjectFile] {
    &[
        DemoProjectFile {
            relative_path: "project.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/project.json"),
        },
        DemoProjectFile {
            relative_path: "button.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/button.json"),
        },
        DemoProjectFile {
            relative_path: "clock.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/clock.json"),
        },
        DemoProjectFile {
            relative_path: "fixture.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/fixture.json"),
        },
        DemoProjectFile {
            relative_path: "output.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/output.json"),
        },
        DemoProjectFile {
            relative_path: "playlist.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/playlist.json"),
        },
        DemoProjectFile {
            relative_path: "radio.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/radio.json"),
        },
        DemoProjectFile {
            relative_path: "idle.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/idle.json"),
        },
        DemoProjectFile {
            relative_path: "idle.glsl",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/idle.glsl"),
        },
        DemoProjectFile {
            relative_path: "blast.json",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/blast.json"),
        },
        DemoProjectFile {
            relative_path: "blast.glsl",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/blast.glsl"),
        },
        DemoProjectFile {
            relative_path: "fyeah-mapping.svg",
            bytes: include_bytes!("../../../../../examples/fyeah-sign/fyeah-mapping.svg"),
        },
    ]
}

pub fn demo_project_deploy_files() -> Vec<ProjectDeployFile> {
    demo_project_files()
        .iter()
        .map(|file| ProjectDeployFile::new(file.relative_path, file.bytes.to_vec()))
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
            files
                .iter()
                .any(|file| file.relative_path == "playlist.json"),
            "fyeah-sign demo must include the playlist node"
        );
        assert_eq!(
            files
                .iter()
                .find(|file| file.relative_path == "project.json")
                .unwrap()
                .bytes,
            include_bytes!("../../../../../examples/fyeah-sign/project.json")
        );
    }
}
