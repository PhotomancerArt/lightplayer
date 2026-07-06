//! Browser tests for the memory-primary store: the reload story at
//! infrastructure level, plus the M1-over-store integration the roadmap
//! rests on.

#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use lpa_fs_opfs::{LpFsOpfs, open_dir, opfs_root, remove_path, run_flush_loop};
use lpfs::{LpFs, LpPath};
use wasm_bindgen_test::*;
use web_sys::FileSystemDirectoryHandle;

wasm_bindgen_test_configure!(run_in_browser);

async fn fresh_test_dir(name: &str) -> FileSystemDirectoryHandle {
    let root = opfs_root().await.expect("opfs root");
    let _ = remove_path(&root, LpPath::new(&format!("/{name}"))).await;
    open_dir(&root, name, true).await.expect("test dir")
}

#[wasm_bindgen_test]
async fn mount_edit_flush_remount() {
    let dir = fresh_test_dir("s-reload").await;

    let store = LpFsOpfs::mount(dir.clone()).await.unwrap();
    store
        .write_file(LpPath::new("/packages/x/project.json"), b"{\"v\":1}")
        .unwrap();
    store
        .write_file(LpPath::new("/packages/x/shader.glsl"), b"void main() {}")
        .unwrap();
    store
        .write_file(LpPath::new("/packages/x/tmp.txt"), b"gone soon")
        .unwrap();
    store
        .delete_file(LpPath::new("/packages/x/tmp.txt"))
        .unwrap();
    let report = store.flush().await.unwrap();
    assert_eq!(report.files_written, 2);

    // the reload: a fresh mount of the same OPFS dir
    let store2 = LpFsOpfs::mount(dir).await.unwrap();
    assert_eq!(
        store2
            .read_file(LpPath::new("/packages/x/project.json"))
            .unwrap(),
        b"{\"v\":1}"
    );
    assert_eq!(
        store2
            .read_file(LpPath::new("/packages/x/shader.glsl"))
            .unwrap(),
        b"void main() {}"
    );
    assert!(
        !store2
            .file_exists(LpPath::new("/packages/x/tmp.txt"))
            .unwrap()
    );
    // a fresh mount starts clean
    assert!(!store2.has_dirty());
}

#[wasm_bindgen_test]
async fn delete_of_flushed_file_propagates() {
    let dir = fresh_test_dir("s-delete").await;
    let store = LpFsOpfs::mount(dir.clone()).await.unwrap();
    store.write_file(LpPath::new("/a.txt"), b"a").unwrap();
    store.flush().await.unwrap();

    store.delete_file(LpPath::new("/a.txt")).unwrap();
    let report = store.flush().await.unwrap();
    assert_eq!(report.paths_removed, 1);

    let store2 = LpFsOpfs::mount(dir).await.unwrap();
    assert!(!store2.file_exists(LpPath::new("/a.txt")).unwrap());
}

#[wasm_bindgen_test]
async fn burst_coalesces_to_one_write() {
    let dir = fresh_test_dir("s-coalesce").await;
    let store = LpFsOpfs::mount(dir).await.unwrap();
    for i in 0..20 {
        store
            .write_file(LpPath::new("/hot.txt"), format!("v{i}").as_bytes())
            .unwrap();
    }
    let report = store.flush().await.unwrap();
    assert_eq!(report.files_written, 1);
    assert_eq!(store.read_file(LpPath::new("/hot.txt")).unwrap(), b"v19");
}

#[wasm_bindgen_test]
async fn watermark_is_honest() {
    let dir = fresh_test_dir("s-watermark").await;
    let store = LpFsOpfs::mount(dir).await.unwrap();
    assert!(!store.has_dirty());

    store.write_file(LpPath::new("/a.txt"), b"1").unwrap();
    assert!(store.has_dirty());
    store.flush().await.unwrap();
    assert!(!store.has_dirty());

    store.write_file(LpPath::new("/b.txt"), b"2").unwrap();
    let report = store.flush().await.unwrap();
    assert_eq!(report.files_written, 1); // only the delta
}

#[wasm_bindgen_test]
async fn chroot_view_changes_reach_the_flusher() {
    // The load-bearing subtlety: writes through a chroot view must land in
    // the store's shared change log (LpFsMemory's own chroot would fork it).
    let dir = fresh_test_dir("s-chroot").await;
    let store = LpFsOpfs::mount(dir.clone()).await.unwrap();

    let view = store.chroot(LpPath::new("/history/prj_x")).unwrap();
    view.borrow()
        .write_file(LpPath::new("/events.jsonl"), b"{}\n")
        .unwrap();

    assert!(store.has_dirty());
    store.flush().await.unwrap();

    let store2 = LpFsOpfs::mount(dir).await.unwrap();
    assert_eq!(
        store2
            .read_file(LpPath::new("/history/prj_x/events.jsonl"))
            .unwrap(),
        b"{}\n"
    );
}

#[wasm_bindgen_test]
async fn flush_loop_drains_in_background() {
    let dir = fresh_test_dir("s-loop").await;
    let store = LpFsOpfs::mount(dir.clone()).await.unwrap();
    wasm_bindgen_futures::spawn_local(run_flush_loop(store.clone(), 25));

    store
        .write_file(LpPath::new("/bg.txt"), b"background")
        .unwrap();
    TimeoutFuture::new(200).await;

    assert!(!store.has_dirty());
    let store2 = LpFsOpfs::mount(dir).await.unwrap();
    assert_eq!(
        store2.read_file(LpPath::new("/bg.txt")).unwrap(),
        b"background"
    );
}

#[wasm_bindgen_test]
async fn lpc_history_runs_over_the_store() {
    // M1-over-store: SnapshotStore + EventLog over a chroot of the store,
    // flushed, remounted, read back.
    use lpc_history::{EventKind, EventLog, HistoryEvent, ProjectHistory, SnapshotStore};

    let dir = fresh_test_dir("s-history").await;
    let store = LpFsOpfs::mount(dir.clone()).await.unwrap();

    // package content lives in the store too
    store
        .write_file(
            LpPath::new("/packages/x/project.json"),
            b"{\"kind\":\"Project\"}",
        )
        .unwrap();
    store
        .write_file(LpPath::new("/packages/x/shader.glsl"), b"void main() {}")
        .unwrap();

    let package_view = store.chroot(LpPath::new("/packages/x")).unwrap();
    let history_view = store.chroot(LpPath::new("/history/prj_x")).unwrap();

    let (package_hash, version) = {
        let history_fs = history_view.borrow();
        let snapshots = SnapshotStore::new(&*history_fs);
        let package_fs = package_view.borrow();
        let (hash, _) = snapshots.put_package(&*package_fs).unwrap();

        let log = EventLog::new(&*history_fs);
        let origin = HistoryEvent {
            at: 1.0,
            kind: EventKind::Created,
        };
        log.append(&origin).unwrap();
        let mut history = ProjectHistory::new(origin).unwrap();
        log.append(&history.record_save(hash, 2.0)).unwrap();
        (hash, hash)
    };
    assert_eq!(package_hash, version);
    store.flush().await.unwrap();

    // reload: everything reconstructs from OPFS alone
    let store2 = LpFsOpfs::mount(dir).await.unwrap();
    let history_view2 = store2.chroot(LpPath::new("/history/prj_x")).unwrap();
    let history_fs2 = history_view2.borrow();

    let log2 = EventLog::new(&*history_fs2);
    let history2 = ProjectHistory::load(&log2).unwrap();
    assert_eq!(history2.head(), Some(package_hash));

    let snapshots2 = SnapshotStore::new(&*history_fs2);
    let manifest = snapshots2.get_tree(&package_hash).unwrap();
    assert_eq!(manifest.entries().len(), 2);

    // materialize the saved version into a fresh area of the store
    let restore_view = store2.chroot(LpPath::new("/packages/restored")).unwrap();
    snapshots2
        .materialize(&package_hash, &*restore_view.borrow())
        .unwrap();
    assert_eq!(
        store2
            .read_file(LpPath::new("/packages/restored/shader.glsl"))
            .unwrap(),
        b"void main() {}"
    );
}
