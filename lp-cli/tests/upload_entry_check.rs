//! `lp-cli upload` checks every playlist entry before it sends a single
//! file (D19): the device only loads the playing entry, so a broken
//! *dormant* pattern would otherwise only surface after it was picked.
//!
//! `projects/test/button-playlist` has two entries: `idle` (1, the idle
//! entry — resident) and `active` (2 — dormant until the button fires).
//! These tests corrupt the dormant entry's own def file and confirm
//! `handle_upload` refuses before it ever connects, naming the entry.

use std::path::{Path, PathBuf};

use lp_cli::commands::upload::{UploadArgs, handle_upload};
use tempfile::TempDir;

#[test]
fn upload_refuses_when_a_dormant_entry_fails_to_load() {
    let (_dir, project_dir) = button_playlist_project();
    std::fs::write(project_dir.join("active.json"), "{ this is not valid json")
        .expect("corrupt the dormant entry's def");

    let result = handle_upload(UploadArgs {
        dir: project_dir,
        host: "local".to_string(),
        no_wait: false,
        wait_timeout_secs: 10,
    });

    let error =
        result.expect_err("a broken dormant entry must refuse the upload, not deploy it");
    let message = format!("{error:#}");
    assert!(
        message.contains("Refusing to upload"),
        "should refuse before deploying, got: {message}"
    );
    assert!(
        message.contains("entry 2") && message.contains("\"active\""),
        "should name the broken entry by display name, got: {message}"
    );
}

#[test]
fn upload_still_succeeds_when_every_entry_is_fine() {
    let (_dir, project_dir) = button_playlist_project();

    let result = handle_upload(UploadArgs {
        dir: project_dir,
        host: "local".to_string(),
        no_wait: false,
        wait_timeout_secs: 10,
    });

    assert!(
        result.is_ok(),
        "an unmodified project (idle entry resident, active entry dormant \
         but fine) must upload normally: {result:?}"
    );
}

/// Copy `projects/test/button-playlist` into a fresh temp dir.
fn button_playlist_project() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("tempdir");
    let project_dir = temp_dir.path().join("project");
    std::fs::create_dir_all(&project_dir).expect("create project dir");

    let source_dir = workspace_dir().join("projects/test/button-playlist");
    for entry in std::fs::read_dir(&source_dir).expect("read button-playlist") {
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

    (temp_dir, project_dir)
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}
