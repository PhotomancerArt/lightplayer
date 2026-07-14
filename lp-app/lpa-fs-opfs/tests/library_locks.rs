//! Browser tests for the typed library lock model and the per-scope mount
//! primitives.
//!
//! Web Locks are origin-wide, not per-tab, so one test context can both
//! hold a lock and observe the refusal a second holder (in product terms:
//! another tab) would get. Release travels through the lock manager
//! asynchronously, so re-acquisition after release polls briefly instead
//! of asserting on the very next task.

#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use lpa_fs_opfs::{
    LibraryLock, LibraryLockGuard, LpFsOpfs, held_project_uids, open_dir, open_library_subdir,
    opfs_root, remove_path, try_acquire, write_file,
};
use lpfs::{LpFs, LpPath};
use wasm_bindgen_test::*;
use web_sys::FileSystemDirectoryHandle;

wasm_bindgen_test_configure!(run_in_browser);

/// Poll `try_acquire` until the lock manager has processed a release.
async fn acquire_eventually(lock: &LibraryLock) -> LibraryLockGuard {
    for _ in 0..50 {
        if let Some(guard) = try_acquire(lock).await.expect("web locks available") {
            return guard;
        }
        TimeoutFuture::new(10).await;
    }
    panic!("lock {} never became available", lock.name());
}

#[wasm_bindgen_test]
async fn released_lock_can_be_reacquired() {
    let lock = LibraryLock::Project("prj_test_release".to_string());

    let guard = try_acquire(&lock).await.unwrap().expect("first acquire");
    assert!(
        try_acquire(&lock).await.unwrap().is_none(),
        "second acquire must be refused while held"
    );

    guard.release();
    let reacquired = acquire_eventually(&lock).await;
    reacquired.release();
}

#[wasm_bindgen_test]
async fn drop_releases_the_lock() {
    let lock = LibraryLock::Project("prj_test_drop".to_string());
    {
        let _guard = try_acquire(&lock).await.unwrap().expect("acquire");
        assert!(try_acquire(&lock).await.unwrap().is_none());
    }
    let reacquired = acquire_eventually(&lock).await;
    reacquired.release();
}

#[wasm_bindgen_test]
async fn catalog_and_project_locks_do_not_conflict() {
    let catalog = try_acquire(&LibraryLock::Catalog).await.unwrap();
    let project = try_acquire(&LibraryLock::Project("prj_test_disjoint".to_string()))
        .await
        .unwrap();
    assert!(catalog.is_some());
    assert!(project.is_some());
}

#[wasm_bindgen_test]
async fn query_lists_held_project_locks() {
    let uid = "prj_test_query";
    let guard = try_acquire(&LibraryLock::Project(uid.to_string()))
        .await
        .unwrap()
        .expect("acquire");

    let held = held_project_uids().await;
    assert!(held.iter().any(|u| u == uid), "held: {held:?}");

    guard.release();
    let mut still_held = true;
    for _ in 0..50 {
        still_held = held_project_uids().await.iter().any(|u| u == uid);
        if !still_held {
            break;
        }
        TimeoutFuture::new(10).await;
    }
    assert!(!still_held, "released lock must leave the query results");
}

async fn fresh_test_dir(name: &str) -> FileSystemDirectoryHandle {
    let root = opfs_root().await.expect("opfs root");
    let _ = remove_path(&root, LpPath::new(&format!("/{name}"))).await;
    open_dir(&root, name, true).await.expect("test dir")
}

#[wasm_bindgen_test]
async fn filtered_mount_skips_rejected_subtrees() {
    let dir = fresh_test_dir("s-filtered").await;
    write_file(&dir, LpPath::new("/packages/x/project.json"), b"{}")
        .await
        .unwrap();
    write_file(&dir, LpPath::new("/history/prj_x/events.jsonl"), b"{}\n")
        .await
        .unwrap();
    write_file(&dir, LpPath::new("/history/prj_x/blobs/abc"), b"payload")
        .await
        .unwrap();
    write_file(&dir, LpPath::new("/history/prj_x/trees/def.json"), b"{}")
        .await
        .unwrap();

    let snapshot = LpFsOpfs::mount_filtered(dir, |path| {
        path.ends_with("/blobs") || path.ends_with("/trees")
    })
    .await
    .unwrap();

    // kept: manifests and event logs
    assert!(
        snapshot
            .file_exists(LpPath::new("/packages/x/project.json"))
            .unwrap()
    );
    assert!(
        snapshot
            .file_exists(LpPath::new("/history/prj_x/events.jsonl"))
            .unwrap()
    );
    // skipped before descending: payload subtrees
    assert!(
        !snapshot
            .file_exists(LpPath::new("/history/prj_x/blobs/abc"))
            .unwrap()
    );
    assert!(
        !snapshot
            .file_exists(LpPath::new("/history/prj_x/trees/def.json"))
            .unwrap()
    );
}

#[wasm_bindgen_test]
async fn library_subdir_round_trips_a_write() {
    let subdir = open_library_subdir("/packages/s-subdir-test", true)
        .await
        .unwrap();
    let store = LpFsOpfs::mount(subdir.clone()).await.unwrap();
    store
        .write_file(LpPath::new("/project.json"), b"{\"v\":1}")
        .unwrap();
    store.flush().await.unwrap();

    // a fresh open of the same subdir sees the write
    let again = open_library_subdir("/packages/s-subdir-test", false)
        .await
        .unwrap();
    let store2 = LpFsOpfs::mount(again).await.unwrap();
    assert_eq!(
        store2.read_file(LpPath::new("/project.json")).unwrap(),
        b"{\"v\":1}"
    );

    // cleanup: husk dirs confuse later runs
    let root = opfs_root().await.unwrap();
    let _ = remove_path(
        &root,
        LpPath::new("/lightplayer-library/packages/s-subdir-test"),
    )
    .await;
}
