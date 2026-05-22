//! Shared fixtures for integration tests.

#![allow(
    dead_code,
    reason = "shared fixtures; not every integration test binary uses all helpers"
)]

use lpfs::{LpFsMemory, LpPath};

pub fn write_file(fs: &mut LpFsMemory, path: &str, contents: &str) {
    fs.write_file_mut(LpPath::new(path), contents.as_bytes())
        .unwrap();
}

pub fn load_shader_project() -> LpFsMemory {
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/shader.toml",
        r#"
kind = "Shader"
source = { path = "shader.glsl" }
render_order = 0
"#,
    );
    write_file(
        &mut fs,
        "/shader.glsl",
        "void main() { gl_FragColor = vec4(1.0); }",
    );
    fs
}

#[allow(
    dead_code,
    reason = "shared fixture; not every integration test binary uses all helpers"
)]
pub fn load_fixture_project() -> LpFsMemory {
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/fixture.toml",
        r#"
kind = "Fixture"
color_order = "rgb"
sampling = "direct"
transform = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]

[render_size]
width = 16
height = 16

[mapping]
kind = "SvgPath"
source = "./mapping.svg"
sample_diameter = 2.0
"#,
    );
    write_file(
        &mut fs,
        "/mapping.svg",
        r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0"/></svg>"#,
    );
    fs
}

#[allow(
    dead_code,
    reason = "shared fixture; not every integration test binary uses all helpers"
)]
pub fn load_playlist_with_inline_child() -> LpFsMemory {
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/playlist.toml",
        r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "a.glsl" }
"#,
    );
    write_file(&mut fs, "/a.glsl", "void main() {}");
    fs
}

pub fn load_clock() -> LpFsMemory {
    let mut fs = LpFsMemory::new();
    write_file(
        &mut fs,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 1.0
"#,
    );
    fs
}
