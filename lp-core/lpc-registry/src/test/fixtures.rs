//! Shared TOML fixtures for fs-change scenario tests.

use lpfs::{LpFsMemory, LpPath};

pub fn write_file(fs: &mut LpFsMemory, path: &str, contents: &str) {
    fs.write_file_mut(LpPath::new(path), contents.as_bytes())
        .unwrap();
}

/// Write the `/project.json` container manifest every loadable project
/// needs post-mitosis (the root module node lives in `/module.json`).
pub fn write_container_manifest(fs: &mut LpFsMemory) {
    write_file(fs, "/project.json", "{\n  \"format\": 4\n}\n");
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
