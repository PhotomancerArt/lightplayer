use lpa_client::ProjectDeployFile;

use crate::STUDIO_DEMO_PROJECT_ID;

pub const DEMO_PROJECT_ID: &str = STUDIO_DEMO_PROJECT_ID;

pub struct DemoProjectFile {
    pub relative_path: &'static str,
    pub bytes: &'static [u8],
}

pub fn demo_project_files() -> &'static [DemoProjectFile] {
    &[
        DemoProjectFile {
            relative_path: "clock.toml",
            bytes: include_bytes!("../../../../lp-fw/fw-browser/www/smoke-project/clock.toml"),
        },
        DemoProjectFile {
            relative_path: "fixture.toml",
            bytes: include_bytes!("../../../../lp-fw/fw-browser/www/smoke-project/fixture.toml"),
        },
        DemoProjectFile {
            relative_path: "output.toml",
            bytes: include_bytes!("../../../../lp-fw/fw-browser/www/smoke-project/output.toml"),
        },
        DemoProjectFile {
            relative_path: "project.toml",
            bytes: include_bytes!("../../../../lp-fw/fw-browser/www/smoke-project/project.toml"),
        },
        DemoProjectFile {
            relative_path: "shader.glsl",
            bytes: include_bytes!("../../../../lp-fw/fw-browser/www/smoke-project/shader.glsl"),
        },
        DemoProjectFile {
            relative_path: "shader.toml",
            bytes: include_bytes!("../../../../lp-fw/fw-browser/www/smoke-project/shader.toml"),
        },
    ]
}

pub fn demo_project_deploy_files() -> Vec<ProjectDeployFile> {
    demo_project_files()
        .iter()
        .map(|file| ProjectDeployFile::new(file.relative_path, file.bytes.to_vec()))
        .collect()
}
