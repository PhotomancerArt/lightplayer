//! Browser tests for the store lock.
//!
//! Web Locks are origin-wide, not per-tab, so one test context can both
//! hold a lock and observe the refusal a second holder (in product terms:
//! another tab) would get.

#![cfg(target_arch = "wasm32")]

use lpa_fs_opfs::acquire_exclusive_lock;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn second_acquisition_is_refused() {
    let first = acquire_exclusive_lock("lp-test-lock-refusal")
        .await
        .unwrap();
    assert!(first, "first acquisition should succeed");

    let second = acquire_exclusive_lock("lp-test-lock-refusal")
        .await
        .unwrap();
    assert!(!second, "second acquisition should be refused while held");
}

#[wasm_bindgen_test]
async fn distinct_keys_do_not_conflict() {
    let a = acquire_exclusive_lock("lp-test-lock-a").await.unwrap();
    let b = acquire_exclusive_lock("lp-test-lock-b").await.unwrap();
    assert!(a);
    assert!(b);
}
