//! Browser tests for the raw OPFS operations.
//!
//! Each test works in its own subdirectory of the OPFS root — OPFS persists
//! across tests within one browser session, so tests must not share paths.

#![cfg(target_arch = "wasm32")]

use lpa_fs_opfs::{load_tree, open_dir, opfs_root, remove_path, write_file};
use lpfs::LpPath;
use wasm_bindgen_test::*;
use web_sys::FileSystemDirectoryHandle;

wasm_bindgen_test_configure!(run_in_browser);

async fn fresh_test_dir(name: &str) -> FileSystemDirectoryHandle {
    let root = opfs_root().await.expect("opfs root");
    // best-effort cleanup of a previous run's leftovers
    let _ = remove_path(&root, LpPath::new(&format!("/{name}"))).await;
    open_dir(&root, name, true).await.expect("test dir")
}

fn tree_map(tree: Vec<(lpfs::LpPathBuf, Vec<u8>)>) -> std::collections::BTreeMap<String, Vec<u8>> {
    tree.into_iter()
        .map(|(p, b)| (p.as_str().to_string(), b))
        .collect()
}

#[wasm_bindgen_test]
async fn write_load_round_trip() {
    let dir = fresh_test_dir("t-round-trip").await;

    write_file(&dir, LpPath::new("/project.json"), b"{}")
        .await
        .unwrap();
    write_file(&dir, LpPath::new("/shader.glsl"), b"void main() {}")
        .await
        .unwrap();
    write_file(
        &dir,
        LpPath::new("/modules/plasma/module.json"),
        b"{\"kind\":\"Module\"}",
    )
    .await
    .unwrap();

    let tree = tree_map(load_tree(&dir).await.unwrap());
    assert_eq!(tree.len(), 3);
    assert_eq!(tree["/project.json"], b"{}");
    assert_eq!(tree["/shader.glsl"], b"void main() {}");
    assert_eq!(
        tree["/modules/plasma/module.json"],
        b"{\"kind\":\"Module\"}"
    );

    // re-open the directory handle: contents persist within the session
    let root = opfs_root().await.unwrap();
    let reopened = open_dir(&root, "t-round-trip", false).await.unwrap();
    let tree2 = tree_map(load_tree(&reopened).await.unwrap());
    assert_eq!(tree2.len(), 3);
    assert_eq!(
        tree2["/modules/plasma/module.json"],
        b"{\"kind\":\"Module\"}"
    );
}

#[wasm_bindgen_test]
async fn overwrite_shrinks() {
    let dir = fresh_test_dir("t-overwrite").await;
    write_file(
        &dir,
        LpPath::new("/a.txt"),
        b"a much longer original content",
    )
    .await
    .unwrap();
    write_file(&dir, LpPath::new("/a.txt"), b"short")
        .await
        .unwrap();
    let tree = tree_map(load_tree(&dir).await.unwrap());
    assert_eq!(tree["/a.txt"], b"short");
}

#[wasm_bindgen_test]
async fn remove_files_and_dirs() {
    let dir = fresh_test_dir("t-remove").await;
    write_file(&dir, LpPath::new("/keep.txt"), b"k")
        .await
        .unwrap();
    write_file(&dir, LpPath::new("/gone.txt"), b"g")
        .await
        .unwrap();
    write_file(&dir, LpPath::new("/sub/nested/deep.txt"), b"d")
        .await
        .unwrap();

    remove_path(&dir, LpPath::new("/gone.txt")).await.unwrap();
    remove_path(&dir, LpPath::new("/sub")).await.unwrap();

    let tree = tree_map(load_tree(&dir).await.unwrap());
    assert_eq!(tree.len(), 1);
    assert!(tree.contains_key("/keep.txt"));
}

#[wasm_bindgen_test]
async fn empty_dir_loads_empty() {
    let dir = fresh_test_dir("t-empty").await;
    let tree = load_tree(&dir).await.unwrap();
    assert!(tree.is_empty());
}

#[wasm_bindgen_test]
async fn remove_missing_errors() {
    let dir = fresh_test_dir("t-remove-missing").await;
    let result = remove_path(&dir, LpPath::new("/nope.txt")).await;
    assert!(result.is_err());
}

/// The write must persist exactly the bytes passed — no view-length
/// confusion. Guards the WebKit whole-buffer bug (`write()` on a wasm
/// memory view wrote the entire heap): with the JS-owned copy in
/// `write_file` this size is exact on every engine.
#[wasm_bindgen_test]
async fn write_persists_exact_length() {
    let dir = fresh_test_dir("t-exact-length").await;
    let payload = vec![0x42u8; 4096];
    write_file(&dir, LpPath::new("/exact.bin"), &payload)
        .await
        .unwrap();
    let tree = tree_map(load_tree(&dir).await.unwrap());
    assert_eq!(tree["/exact.bin"].len(), payload.len());
    assert_eq!(tree["/exact.bin"], payload);
}

/// Corrupt-store recovery: a file no legitimate package could contain
/// (pre-fix WebKit wrote the whole wasm heap into every file) is skipped
/// by the mount instead of being loaded into memory.
#[wasm_bindgen_test]
async fn load_tree_skips_implausibly_large_files() {
    let dir = fresh_test_dir("t-oversize").await;
    write_file(&dir, LpPath::new("/ok.json"), b"{}")
        .await
        .unwrap();
    // 17 MiB of zeros — just over the 16 MiB cap.
    let huge = vec![0u8; 17 * 1024 * 1024];
    write_file(&dir, LpPath::new("/heap-dump.json"), &huge)
        .await
        .unwrap();

    let tree = tree_map(load_tree(&dir).await.unwrap());
    assert!(tree.contains_key("/ok.json"));
    assert!(
        !tree.contains_key("/heap-dump.json"),
        "oversized file must be skipped, not loaded"
    );
}
