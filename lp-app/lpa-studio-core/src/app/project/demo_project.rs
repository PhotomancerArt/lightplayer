use lpa_client::ProjectDeployFile;

use crate::STUDIO_DEMO_PROJECT_ID;

pub const DEMO_PROJECT_ID: &str = STUDIO_DEMO_PROJECT_ID;
pub const DEMO_PROJECT_STORAGE_ID: &str = "studio";

pub struct DemoProjectFile {
    pub relative_path: &'static str,
    pub bytes: &'static [u8],
}

pub fn demo_project_files() -> &'static [DemoProjectFile] {
    &[
        DemoProjectFile {
            relative_path: "clock.toml",
            bytes: include_bytes!("../../../../../examples/basic/clock.toml"),
        },
        DemoProjectFile {
            relative_path: "fixture.toml",
            bytes: include_bytes!("../../../../../examples/basic/fixture.toml"),
        },
        DemoProjectFile {
            relative_path: "output.toml",
            bytes: include_bytes!("../../../../../examples/basic/output.toml"),
        },
        DemoProjectFile {
            relative_path: "project.toml",
            bytes: include_bytes!("../../../../../examples/basic/project.toml"),
        },
        DemoProjectFile {
            relative_path: "shader.glsl",
            bytes: include_bytes!("../../../../../examples/basic/shader.glsl"),
        },
        DemoProjectFile {
            relative_path: "shader.toml",
            bytes: include_bytes!("../../../../../examples/basic/shader.toml"),
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
    fn demo_project_identity_uses_examples_basic() {
        assert_eq!(DEMO_PROJECT_ID, "examples/basic");
        assert_eq!(DEMO_PROJECT_STORAGE_ID, "studio");
    }

    #[test]
    fn demo_project_files_are_the_basic_example() {
        let files = demo_project_files();

        assert_eq!(
            files
                .iter()
                .map(|file| file.relative_path)
                .collect::<Vec<_>>(),
            vec![
                "clock.toml",
                "fixture.toml",
                "output.toml",
                "project.toml",
                "shader.glsl",
                "shader.toml",
            ]
        );
        assert_eq!(
            files
                .iter()
                .find(|file| file.relative_path == "project.toml")
                .unwrap()
                .bytes,
            include_bytes!("../../../../../examples/basic/project.toml")
        );
    }
}
