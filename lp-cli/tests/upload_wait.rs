//! `lp-cli upload` waits for evidence the newly deployed project is
//! running before it disconnects.
//!
//! Coverage for P5 of `docs/defects/2026-07-30-deploy-compiles-previous-upload.md`:
//! `upload` used to disconnect the instant `LoadProject` was acked, so every
//! compile line an operator could observe described the *previous* upload.
//!
//! These tests drive the real `handle_upload` entry point (not a stand-in)
//! against `HostSpecifier::Local` — an in-process `fw-host` runtime that
//! ticks a real `LpServer` on its own thread over an in-memory transport
//! pair (`lp-cli/src/client/host_process.rs`). That gives end-to-end
//! coverage of the wait/poll loop without a real serial device.

use std::path::{Path, PathBuf};

use lp_cli::commands::upload::{UploadArgs, handle_upload};
use tempfile::TempDir;

/// A shader with a syntax error naga's GLSL front end will reject.
const BROKEN_SHADER: &str = r#"
layout(binding = 0) uniform vec2 outputSize;

vec4 render_2d(vec2 pos) {
    this is not glsl;
}
"#;

#[test]
fn upload_waits_and_reports_the_project_running() {
    let (_dir, project_dir) = shader_oracle_project(None);

    let result = handle_upload(UploadArgs {
        dir: project_dir,
        host: "local".to_string(),
        no_wait: false,
        wait_timeout_secs: 10,
    });

    assert!(
        result.is_ok(),
        "upload should wait for the project to render and then succeed: {result:?}"
    );
}

#[test]
fn upload_wait_ends_nonzero_on_a_shader_compile_failure() {
    let (_dir, project_dir) = shader_oracle_project(Some(BROKEN_SHADER));

    let result = handle_upload(UploadArgs {
        dir: project_dir,
        host: "local".to_string(),
        no_wait: false,
        wait_timeout_secs: 10,
    });

    let error = result.expect_err(
        "a shader that fails to compile must end the wait with an error, not a timeout",
    );
    let message = format!("{error:#}");
    assert!(
        message.contains("failed to run"),
        "error should say the deployed project failed to run, got: {message}"
    );
    // The wait must recognize a compile failure immediately rather than
    // spin until the timeout: this assertion is meaningless if the failure
    // path silently degenerated into "waited the full 10s and gave up",
    // so a slow-but-passing run here would be its own bug.
}

#[test]
fn no_wait_skips_the_wait_even_when_the_shader_would_fail() {
    let (_dir, project_dir) = shader_oracle_project(Some(BROKEN_SHADER));

    let result = handle_upload(UploadArgs {
        dir: project_dir,
        host: "local".to_string(),
        no_wait: true,
        wait_timeout_secs: 10,
    });

    assert!(
        result.is_ok(),
        "--no-wait must restore fire-and-forget: a deploy that was acked succeeds \
         even though the shader will fail to compile after this call returns: {result:?}"
    );
}

#[test]
fn upload_wait_times_out_nonzero_when_no_evidence_arrives() {
    let (_dir, project_dir) = shader_oracle_project(None);

    let result = handle_upload(UploadArgs {
        dir: project_dir,
        host: "local".to_string(),
        no_wait: false,
        // A budget too small for a real render loop tick (~16ms/frame) to
        // ever land a poll response: proves the timeout path is reachable
        // and reports what it was waiting for.
        wait_timeout_secs: 0,
    });

    let error = result.expect_err("a zero-second budget must time out, not hang or succeed");
    let message = format!("{error:#}");
    assert!(
        message.contains("no evidence") && message.contains("acked"),
        "timeout error should say the deploy was acked but running evidence never arrived, got: {message}"
    );
}

/// Copy `examples/shader-oracle` into a fresh temp dir, optionally
/// overwriting `shader.glsl` with `shader_override`.
fn shader_oracle_project(shader_override: Option<&str>) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("tempdir");
    let project_dir = temp_dir.path().join("project");
    std::fs::create_dir_all(&project_dir).expect("create project dir");

    let source_dir = workspace_dir().join("examples").join("shader-oracle");
    for entry in std::fs::read_dir(&source_dir).expect("read shader-oracle") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().expect("file name");
        if name == "README.md" {
            continue;
        }
        std::fs::copy(&path, project_dir.join(name)).expect("copy project file");
    }

    if let Some(shader) = shader_override {
        std::fs::write(project_dir.join("shader.glsl"), shader).expect("overwrite shader");
    }

    (temp_dir, project_dir)
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}
