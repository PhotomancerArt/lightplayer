//! Browser tests for the typed library lock model and the per-scope mount
//! primitives.
//!
//! Web Locks are origin-wide, not per-tab, so one test context can both
//! hold a lock and observe the refusal a second holder (in product terms:
//! another tab) would get. Release travels through the lock manager
//! asynchronously, so re-acquisition after release polls briefly instead
//! of asserting on the very next task.

#![cfg(target_arch = "wasm32")]

use std::cell::Cell;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use lpa_fs_opfs::{
    LibraryLock, LibraryLockGuard, LpFsOpfs, held_project_uids, open_dir, open_library_subdir,
    opfs_root, remove_path, try_acquire, try_acquire_polling, write_file,
};
use lpfs::{LpFs, LpPath};
use wasm_bindgen_test::*;
use web_sys::FileSystemDirectoryHandle;

wasm_bindgen_test_configure!(run_in_browser);

/// The open path's acquire ladder, restated: `OPEN_RETRIES` ×
/// `OPEN_RETRY_DELAY_MS` in `lpa-studio-web/src/library_host_opfs.rs`.
/// Restated rather than shared because the *policy* belongs to the caller
/// — this crate only offers the poll.
const OPEN_LADDER_ATTEMPTS: usize = 10;
const OPEN_LADDER_DELAY_MS: u32 = 50;

/// Poll `try_acquire` until the lock manager has processed a release.
async fn acquire_eventually(lock: &LibraryLock) -> LibraryLockGuard {
    try_acquire_polling(lock, 50, 10)
        .await
        .expect("web locks available")
        .unwrap_or_else(|| panic!("lock {} never became available", lock.name()))
}

#[wasm_bindgen_test]
async fn released_lock_can_be_reacquired() {
    let lock = LibraryLock::Project("prjtestrelease".to_string());

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
    let lock = LibraryLock::Project("prjtestdrop".to_string());
    {
        let _guard = try_acquire(&lock).await.unwrap().expect("acquire");
        assert!(try_acquire(&lock).await.unwrap().is_none());
    }
    let reacquired = acquire_eventually(&lock).await;
    reacquired.release();
}

/// The open path's guarantee (P1): a hold that is about to end must not be
/// reported as "open in another tab". The lock manager hands a release on
/// in a later task, so the instant shot loses this race every time and the
/// ladder is what wins it.
#[wasm_bindgen_test]
async fn the_open_ladder_waits_out_a_hold_that_is_ending() {
    let lock = LibraryLock::Project("prjtestpollrelease".to_string());
    let guard = try_acquire(&lock).await.unwrap().expect("first acquire");

    // let go on a later task — a finishing cloud sync trip, or the
    // sim-crash recovery that releases and immediately reopens
    wasm_bindgen_futures::spawn_local(async move {
        TimeoutFuture::new(120).await;
        guard.release();
    });

    assert!(
        try_acquire(&lock).await.unwrap().is_none(),
        "one instant shot loses the race (this is the bug being fixed)"
    );
    let polled = try_acquire_polling(&lock, OPEN_LADDER_ATTEMPTS, OPEN_LADDER_DELAY_MS)
        .await
        .unwrap()
        .expect("the ladder outlasts a hold that ends inside its budget");
    polled.release();
}

/// …and stays bounded: a project a *real* other tab holds still refuses,
/// which is what `OpenElsewhere` is for.
#[wasm_bindgen_test]
async fn the_open_ladder_still_refuses_a_lock_held_throughout() {
    let lock = LibraryLock::Project("prjtestpollrefuse".to_string());
    let guard = try_acquire(&lock).await.unwrap().expect("acquire");

    assert!(
        try_acquire_polling(&lock, 3, 10).await.unwrap().is_none(),
        "the ladder is bounded; exhausting it is the refusal"
    );

    guard.release();
}

/// The other half of P1: an open that fails *after* acquiring must give
/// the project straight back, so the retry a second click makes finds it
/// free. The teardown itself is `OpenRegistry::release_open`
/// (`lpa-studio-web/src/library_host_opfs.rs`), which cannot run here; what
/// this pins is the lock-level guarantee it rests on — a guard that unwinds
/// with the failure leaves nothing behind.
#[wasm_bindgen_test]
async fn a_failed_open_leaves_the_project_reopenable() {
    async fn open_then_fail(lock: &LibraryLock) -> Result<(), &'static str> {
        let _guard = try_acquire_polling(lock, OPEN_LADDER_ATTEMPTS, OPEN_LADDER_DELAY_MS)
            .await
            .expect("web locks available")
            .expect("uncontended acquire");
        // the caller's half of the open fails (worker boot timeout, a
        // refused migration): the guard unwinds with the error, exactly as
        // an abandoned `OpenReceipt` arranges
        Err("timed out waiting for browser worker boot")
    }

    let lock = LibraryLock::Project("prjtestfailedopen".to_string());
    assert!(open_then_fail(&lock).await.is_err());

    let reopened = try_acquire_polling(&lock, OPEN_LADDER_ATTEMPTS, OPEN_LADDER_DELAY_MS)
        .await
        .unwrap()
        .expect("a failed open must not keep the project");
    reopened.release();
}

/// P2/D1, at lock level: a cloud sync trip takes the project lock only to
/// snapshot the project and lets it go *before* the network, so the click a
/// user makes while that trip is still publishing wins — instantly, and
/// well inside the open ladder's budget. Holding across the round trip is
/// what turned a fresh seed's first click into "open in another tab"
/// (`docs/defects/2026-08-14-sync-holds-the-project-lock-across-the-network.md`).
///
/// The trip is played here rather than driven, because `mount_for_sync`
/// (`lpa-studio-web/src/library_host_opfs.rs`) is not reachable from this
/// crate; what is pinned is the shape it now has.
#[wasm_bindgen_test]
async fn a_publishing_sync_trip_does_not_hold_the_project() {
    let lock = LibraryLock::Project("prjtestsyncsnapshot".to_string());
    let publishing = Rc::new(Cell::new(false));

    let trip_lock = lock.clone();
    let trip_publishing = Rc::clone(&publishing);
    wasm_bindgen_futures::spawn_local(async move {
        let guard = try_acquire_polling(&trip_lock, OPEN_LADDER_ATTEMPTS, OPEN_LADDER_DELAY_MS)
            .await
            .expect("web locks available")
            .expect("the trip's snapshot hold");
        // snapshotting: local reads, all of the trip's exposure to the lock
        TimeoutFuture::new(20).await;
        guard.release();
        // …and only now the network, from the snapshot
        trip_publishing.set(true);
        TimeoutFuture::new(2_000).await;
        trip_publishing.set(false);
    });

    while !publishing.get() {
        TimeoutFuture::new(10).await;
    }
    let opened = try_acquire_polling(&lock, OPEN_LADDER_ATTEMPTS, OPEN_LADDER_DELAY_MS)
        .await
        .unwrap()
        .expect("an open must win against a trip that is only publishing");
    assert!(
        publishing.get(),
        "the open has to land while the publish is still in flight, or this proves nothing"
    );
    opened.release();

    // Stronger, and the phrasing the plan uses: the publish half could take
    // the lock itself — one instant shot, no ladder, nothing held.
    let during_publish = acquire_eventually(&lock).await;
    assert!(publishing.get(), "still mid-publish");
    during_publish.release();
}

/// The other half of the snapshot model: the trip publishes from a copy, so
/// its write-back must land *only what the trip itself wrote*. A save that
/// arrived on disk while the publish was in flight is newer than the
/// snapshot, and banking the binding must not carry the stale copy over it.
#[wasm_bindgen_test]
async fn a_snapshot_banks_only_what_the_trip_wrote() {
    let dir = fresh_test_dir("s-snapshot-bank").await;
    write_file(&dir, LpPath::new("/events.jsonl"), b"created\n")
        .await
        .unwrap();

    // under the project lock: read the whole subtree into memory
    let snapshot = LpFsOpfs::mount(dir.clone()).await.unwrap();

    // lock released; the trip runs on the snapshot and writes its binding
    snapshot
        .write_file(LpPath::new("/cloud-binding.json"), b"{\"bound\":true}")
        .unwrap();
    // …while the user opened the project and saved into the same subtree
    write_file(&dir, LpPath::new("/events.jsonl"), b"created\nsaved\n")
        .await
        .unwrap();

    // the bank phase, under the lock again
    let pending: Vec<String> = snapshot
        .pending_writes()
        .into_iter()
        .map(|(path, _)| path.as_str().to_string())
        .collect();
    assert_eq!(
        pending,
        vec!["/cloud-binding.json".to_string()],
        "the snapshot is dirty only where the trip wrote"
    );
    snapshot.flush().await.unwrap();

    let after = LpFsOpfs::mount(dir).await.unwrap();
    assert_eq!(
        after.read_file(LpPath::new("/events.jsonl")).unwrap(),
        b"created\nsaved\n",
        "the mid-publish save survives the bank"
    );
    assert_eq!(
        after.read_file(LpPath::new("/cloud-binding.json")).unwrap(),
        b"{\"bound\":true}"
    );
}

#[wasm_bindgen_test]
async fn catalog_and_project_locks_do_not_conflict() {
    let catalog = try_acquire(&LibraryLock::Catalog).await.unwrap();
    let project = try_acquire(&LibraryLock::Project("prjtestdisjoint".to_string()))
        .await
        .unwrap();
    assert!(catalog.is_some());
    assert!(project.is_some());
}

#[wasm_bindgen_test]
async fn query_lists_held_project_locks() {
    let uid = "prjtestquery";
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
    write_file(&dir, LpPath::new("/history/prjx/events.jsonl"), b"{}\n")
        .await
        .unwrap();
    write_file(&dir, LpPath::new("/history/prjx/blobs/abc"), b"payload")
        .await
        .unwrap();
    write_file(&dir, LpPath::new("/history/prjx/trees/def.json"), b"{}")
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
            .file_exists(LpPath::new("/history/prjx/events.jsonl"))
            .unwrap()
    );
    // skipped before descending: payload subtrees
    assert!(
        !snapshot
            .file_exists(LpPath::new("/history/prjx/blobs/abc"))
            .unwrap()
    );
    assert!(
        !snapshot
            .file_exists(LpPath::new("/history/prjx/trees/def.json"))
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
