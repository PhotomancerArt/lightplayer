//! Browser tests for Web Locks by name: the claim, the polling ladder, the
//! query of held names and the watch (Studio's board holds use all four).
//!
//! Web Locks are origin-wide, not per-tab, so one test context can both
//! hold a lock and observe what a second holder (in product terms: another
//! tab) would see. Release travels through the lock manager asynchronously,
//! so whatever must notice a release polls or awaits instead of asserting
//! on the very next task.

#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use lpa_fs_opfs::{
    LibraryLock, held_lock_names, held_project_uids, try_acquire, try_acquire_named_lock,
    try_acquire_named_lock_polling, watch_lock,
};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn a_named_lock_is_exclusive_and_comes_back_after_release() {
    let name = "lp-board:usb:303a:1001:a0f26287b401";

    let guard = try_acquire_named_lock(name)
        .await
        .expect("web locks available")
        .expect("first acquire");
    assert_eq!(guard.lock_name(), name);
    assert!(
        try_acquire_named_lock(name).await.unwrap().is_none(),
        "a second acquire is refused while it is held"
    );

    guard.release();
    let again = try_acquire_named_lock_polling(name, 50, 10)
        .await
        .unwrap()
        .expect("free after the release");
    drop(again);
    let after_drop = try_acquire_named_lock_polling(name, 50, 10)
        .await
        .unwrap()
        .expect("dropping a guard releases it too");
    after_drop.release();
}

/// The board hold's claim ladder (10 × 50 ms, the library's own): a release
/// made one task earlier still refuses the very next instant shot, and the
/// ladder is what picks it up.
#[wasm_bindgen_test]
async fn polling_picks_up_a_release_made_a_moment_earlier() {
    let name = "lp-board:usb:303a:1001:a0f26287b402";
    let guard = try_acquire_named_lock(name)
        .await
        .unwrap()
        .expect("acquire");

    wasm_bindgen_futures::spawn_local(async move {
        TimeoutFuture::new(60).await;
        guard.release();
    });

    assert!(
        try_acquire_named_lock(name).await.unwrap().is_none(),
        "one instant shot loses the race"
    );
    let polled = try_acquire_named_lock_polling(name, 10, 50)
        .await
        .unwrap()
        .expect("the ladder outlasts a hold that ends inside its budget");
    polled.release();

    // …and stays bounded against a lock held throughout. (Taking it back
    // polls too: the release just above is still on its way through the
    // lock manager.)
    let held = try_acquire_named_lock_polling(name, 50, 10)
        .await
        .unwrap()
        .expect("acquire");
    assert!(
        try_acquire_named_lock_polling(name, 3, 10)
            .await
            .unwrap()
            .is_none()
    );
    held.release();
}

#[wasm_bindgen_test]
async fn held_lock_names_lists_a_held_name_and_not_a_free_one() {
    let held_name = "lp-board:net:a0f26287b403";
    let free_name = "lp-board:net:a0f26287b404";
    let guard = try_acquire_named_lock(held_name)
        .await
        .unwrap()
        .expect("acquire");

    let names = held_lock_names("lp-board:").await.expect("query");
    assert!(names.iter().any(|name| name == held_name), "{names:?}");
    assert!(!names.iter().any(|name| name == free_name), "{names:?}");
    assert!(
        names.iter().all(|name| name.starts_with("lp-board:")),
        "filtered by prefix: {names:?}"
    );

    guard.release();
    let mut still_held = true;
    for _ in 0..50 {
        still_held = held_lock_names("lp-board:")
            .await
            .unwrap()
            .iter()
            .any(|name| name == held_name);
        if !still_held {
            break;
        }
        TimeoutFuture::new(10).await;
    }
    assert!(!still_held, "a released lock leaves the query");
}

/// The sentinel: a watch queues behind the holder, resolves `true` once
/// the holder lets go, and never holds the lock itself afterwards.
#[wasm_bindgen_test]
async fn a_watch_resolves_after_the_holder_lets_go_and_holds_nothing() {
    let name = "lp-board:usb:303a:1001:a0f26287b405";
    let guard = try_acquire_named_lock(name)
        .await
        .unwrap()
        .expect("acquire");
    let watch = watch_lock(name).expect("watch");

    wasm_bindgen_futures::spawn_local(async move {
        TimeoutFuture::new(60).await;
        guard.release();
    });

    assert!(
        watch.granted().await,
        "the watch is granted after the release"
    );
    let claimed = try_acquire_named_lock_polling(name, 10, 50)
        .await
        .unwrap()
        .expect("the watch let go the moment it was granted");
    claimed.release();
}

/// A watch on a lock nobody holds is granted at once.
#[wasm_bindgen_test]
async fn a_watch_on_a_free_lock_is_granted_at_once() {
    let watch = watch_lock("lp-board:net:a0f26287b406").expect("watch");
    assert!(watch.granted().await);
}

/// Cancelling a watch answers `false` and leaves nothing queued: the holder
/// lets go later and the lock is simply free.
#[wasm_bindgen_test]
async fn a_cancelled_watch_answers_false_and_leaves_nothing_behind() {
    let name = "lp-board:usb:303a:1001:a0f26287b407";
    let guard = try_acquire_named_lock(name)
        .await
        .unwrap()
        .expect("acquire");
    let watch = watch_lock(name).expect("watch");

    watch.cancel();
    assert!(!watch.granted().await, "cancelled before the grant");

    guard.release();
    let claimed = try_acquire_named_lock_polling(name, 10, 50)
        .await
        .unwrap()
        .expect("nothing left queued to take it");
    claimed.release();
}

/// A board hold is never a project held in another tab: the gallery's
/// "open in another tab" badge reads `lp-project:` only.
#[wasm_bindgen_test]
async fn a_board_hold_never_shows_up_as_a_held_project() {
    let board = try_acquire_named_lock("lp-board:usb:303a:1001:a0f26287b408")
        .await
        .unwrap()
        .expect("acquire the board's lock");
    let project = try_acquire(&LibraryLock::Project("prjtestnamedboard".to_string()))
        .await
        .unwrap()
        .expect("acquire a project's lock");

    let uids = held_project_uids().await;
    assert!(
        uids.iter().any(|uid| uid == "prjtestnamedboard"),
        "{uids:?}"
    );
    assert!(
        uids.iter().all(|uid| !uid.contains("lp-board")),
        "a board hold leaked into the projects: {uids:?}"
    );

    project.release();
    board.release();
}
