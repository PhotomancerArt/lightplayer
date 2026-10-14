//! What a board says about what it plays (`LpServer::loaded_project_facts`)
//! and the picture of its outputs (`output_picture_lamps`,
//! `append_output_picture`): the cloud relay's `Project` and `Picture`
//! frames are made from these, between ticks, with `&LpServer` only.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use std::path::{Path, PathBuf};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpfs::lp_path::LpPathBuf;

#[test]
fn the_basic_project_says_its_manifests_name_and_no_uid() {
    let mut server = server();
    copy_project(&mut server, "basic", None);
    server
        .load_project("/projects/basic".as_path())
        .expect("basic loads");
    let facts = server.loaded_project_facts().expect("a project is loaded");
    assert_eq!(facts.name, "Basic");
    assert_eq!(facts.uid, None);
}

#[test]
fn a_manifest_with_a_uid_says_it() {
    let mut server = server();
    copy_project(
        &mut server,
        "lamp",
        Some(r#"{"format":11,"uid":"prj7m3qk2x9z4w8v6t5r1n0p2a4c","name":"Rocaille"}"#),
    );
    server
        .load_project("/projects/lamp".as_path())
        .expect("loads");
    let facts = server.loaded_project_facts().expect("loaded");
    assert_eq!(facts.name, "Rocaille");
    assert_eq!(facts.uid, Some("prj7m3qk2x9z4w8v6t5r1n0p2a4c"));
}

#[test]
fn a_manifest_with_no_name_says_the_folders() {
    let mut server = server();
    copy_project(&mut server, "desk-lamp", Some(r#"{"format":11}"#));
    server
        .load_project("/projects/desk-lamp".as_path())
        .expect("loads");
    let facts = server.loaded_project_facts().expect("loaded");
    assert_eq!(facts.name, "desk-lamp");
    assert_eq!(facts.uid, None);
}

#[test]
fn a_renamed_manifest_is_the_new_name_after_the_refresh() {
    let mut server = server();
    copy_project(&mut server, "basic", None);
    server
        .load_project("/projects/basic".as_path())
        .expect("loads");
    server
        .base_fs_mut()
        .write_file(
            "/projects/basic/project.json".as_path(),
            br#"{"format":11,"name":"Porch","uid":"prjaaaabbbbccccddddeeeeffff"}"#,
        )
        .expect("rename");
    server.advance_frame(16).expect("tick");
    let facts = server.loaded_project_facts().expect("loaded");
    assert_eq!(facts.name, "Porch");
    assert_eq!(facts.uid, Some("prjaaaabbbbccccddddeeeeffff"));

    // A manifest that no longer reads keeps the name the project had.
    server
        .base_fs_mut()
        .write_file("/projects/basic/project.json".as_path(), b"{ not json")
        .expect("break it");
    server.advance_frame(16).expect("tick");
    assert_eq!(
        server.loaded_project_facts().map(|facts| facts.name),
        Some("Porch")
    );
}

#[test]
fn nothing_loaded_says_nothing_and_pictures_nothing() {
    let server = server();
    assert!(server.loaded_project_facts().is_none());
    let mut lamps = vec![7];
    server.output_picture_lamps(16, &mut lamps);
    assert!(lamps.is_empty());
    let mut rgb = Vec::new();
    server.append_output_picture(&lamps, 0, &mut rgb);
    assert!(rgb.is_empty());
}

#[test]
fn the_basic_project_after_a_few_ticks_has_lamps_and_a_picture_of_them() {
    let mut server = server();
    copy_project(&mut server, "basic", None);
    server
        .load_project("/projects/basic".as_path())
        .expect("basic loads");
    for _ in 0..4 {
        server.advance_frame(16).expect("tick");
    }
    let mut lamps = Vec::new();
    server.output_picture_lamps(16, &mut lamps);
    let total: u64 = lamps.iter().map(|&lamps| u64::from(lamps)).sum();
    assert!(total > 0, "the basic project publishes lamps: {lamps:?}");
    // `lpc_relay::picture_sample_count(T, DEFAULT_PICTURE_SAMPLES)`.
    let count = total.min(256) as u32;
    let mut rgb = vec![0xee];
    server.append_output_picture(&lamps, count, &mut rgb);
    assert_eq!(rgb.len(), 1 + 3 * count as usize, "appended, 3·count bytes");
    assert!(
        rgb[1..].iter().any(|&byte| byte != 0),
        "the basic project lights something"
    );
}

fn server() -> LpServer {
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    LpServer::new(
        Rc::new(RefCell::new(MemoryOutputProvider::new_permissive())),
        Box::new(lpfs::LpFsMemory::new()),
        "projects/".as_path(),
        None,
        None,
        graphics,
    )
}

/// `projects/test/basic`'s files under `/projects/<name>/`, with
/// `manifest` as its `project.json` when given.
fn copy_project(server: &mut LpServer, name: &str, manifest: Option<&str>) {
    let dir = repo_root().join("projects/test/basic");
    for entry in std::fs::read_dir(&dir).expect("read project dir") {
        let path = entry.expect("dir entry").path();
        if !path.is_file() {
            continue;
        }
        let file = path.file_name().expect("file name").to_string_lossy();
        let bytes = match (file.as_ref(), manifest) {
            ("project.json", Some(manifest)) => manifest.as_bytes().to_vec(),
            _ => std::fs::read(&path).expect("read project file"),
        };
        let target = LpPathBuf::from("/projects").join(name).join(file.as_ref());
        server
            .base_fs_mut()
            .write_file(target.as_path(), &bytes)
            .expect("write file");
    }
}

/// `CARGO_MANIFEST_DIR` is `lp-app/lpa-server`; the repo root is two up.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}
