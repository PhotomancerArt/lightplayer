//! The driven flush loop for [`crate::lp_fs_opfs::LpFsOpfs`].
//!
//! The store itself never schedules anything (sans-IO habits at the edge);
//! the host spawns this loop (`wasm_bindgen_futures::spawn_local`) and owns
//! its lifetime.

use gloo_timers::future::TimeoutFuture;

use crate::lp_fs_opfs::LpFsOpfs;

/// Poll-and-flush loop: every `interval_ms`, flush if dirty.
///
/// A change therefore reaches OPFS within roughly `interval_ms` plus write
/// time; changes arriving during a flush ride the next cycle. Flush errors
/// are logged and retried on the next cycle (the watermark only advances on
/// success). Runs forever — the host owns cancellation by dropping the task
/// (e.g. page teardown).
pub async fn run_flush_loop(fs: LpFsOpfs, interval_ms: u32) {
    loop {
        TimeoutFuture::new(interval_ms).await;
        if fs.has_dirty() {
            if let Err(e) = fs.flush().await {
                log::warn!("opfs flush failed (will retry): {e}");
            }
        }
    }
}
