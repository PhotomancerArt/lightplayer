//! Embedded example projects deployable into preview-lab runtimes.
//!
//! Mirrors the demo-project embedding in `lpa-studio-core`
//! (`app/project/demo_project.rs`): each card's browser runtime gets the
//! full example written over the wire protocol, then loaded.

use lpa_client::ProjectDeployFile;

use crate::exploration::preview_lab_config::LabProject;

struct ExampleFile {
    relative_path: &'static str,
    bytes: &'static [u8],
}

pub(super) fn deploy_files(project: LabProject) -> Vec<ProjectDeployFile> {
    files(project)
        .iter()
        .map(|file| ProjectDeployFile::new(file.relative_path, file.bytes.to_vec()))
        .collect()
}

fn files(project: LabProject) -> &'static [ExampleFile] {
    match project {
        LabProject::Basic => &BASIC,
        LabProject::Fluid => &FLUID,
        LabProject::Events => &EVENTS,
        LabProject::FyeahSign => &FYEAH_SIGN,
    }
}

macro_rules! example_files {
    ($name:ident, $dir:literal, [$($file:literal),+ $(,)?]) => {
        static $name: [ExampleFile; example_files!(@count $($file)+)] = [
            $(ExampleFile {
                relative_path: $file,
                bytes: include_bytes!(concat!("../../../../../examples/", $dir, "/", $file)),
            },)+
        ];
    };
    (@count $($file:literal)+) => { [$(example_files!(@one $file)),+].len() };
    (@one $file:literal) => { () };
}

example_files!(
    BASIC,
    "basic",
    [
        "clock.json",
        "fixture.json",
        "output.json",
        "project.json",
        "shader.glsl",
        "shader.json",
    ]
);

example_files!(
    FLUID,
    "fluid",
    [
        "clock.json",
        "compute.glsl",
        "compute.json",
        "fixture.json",
        "fluid.json",
        "output.json",
        "project.json",
    ]
);

example_files!(
    EVENTS,
    "events",
    [
        "clock.json",
        "event_a.glsl",
        "event_a.json",
        "event_b.glsl",
        "event_b.json",
        "fixture.json",
        "output.json",
        "project.json",
        "shader.glsl",
        "shader.json",
    ]
);

example_files!(
    FYEAH_SIGN,
    "fyeah-sign",
    [
        "blast.glsl",
        "blast.json",
        "button.json",
        "clock.json",
        "fixture.json",
        "fyeah-mapping.svg",
        "idle.glsl",
        "idle.json",
        "output.json",
        "playlist.json",
        "project.json",
        "radio.json",
    ]
);
