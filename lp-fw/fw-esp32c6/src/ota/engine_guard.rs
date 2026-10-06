//! The engine guard (director decision DD34): on a boot whose record carries
//! no `confirmed` mark — the first boot after a USB flash — the core hashes
//! the engine it is about to enter, once, against its digest slot. A
//! mismatch keeps the board core-only (engine-less, so any host holding the
//! engine heals it); a match marks the record confirmed, so a confirmed boot
//! pays nothing (the split image's D20 boot-time argument holds).
//!
//! It is what would have caught the split image's dropped-tail defect
//! (`docs/defects/2026-10-05-the-host-flasher-dropped-the-split-images-last-bytes.md`):
//! a committed header over an engine whose last bytes never reached flash.
//! It hashes on the SHA accelerator ([`super::hw_sha`]).
//!
//! An engine an update installs is hashed before it is committed (DM11), and
//! its trial confirms on its link, so the guard does not run again after an
//! over-the-air update.

use super::ENGINE_VADDR;
use super::boot_state::BootState;
use super::hw_sha;
use super::split_flash::SplitFlash;

/// Hash the mapped engine (`len` bytes behind [`ENGINE_VADDR`]) against
/// `digest`. `Err` names why the core must not enter it.
pub fn engine_guard(
    state: &mut BootState,
    len: u32,
    digest: &[u8; 32],
) -> Result<(), &'static str> {
    let started = embassy_time::Instant::now();
    // SAFETY: `find_engine` mapped exactly `len` bytes of committed engine
    // behind the window; flash reads through the cache.
    let engine = unsafe { core::slice::from_raw_parts(ENGINE_VADDR as *const u8, len as usize) };
    let sha = hw_sha::sha256(engine);
    let ms = started.elapsed().as_millis();
    if &sha != digest {
        log::error!(
            "[OTA] engine guard: the engine ({len} B) does not hash to its digest ({ms} ms) — staying core-only"
        );
        return Err("the engine does not hash to its digest");
    }
    log::info!("[OTA] engine guard: the engine ({len} B) matches its digest ({ms} ms)");
    if let Some(layout) = state.layout {
        let mut flash = SplitFlash::take();
        flash.protect(state.core_extent(), layout.region_end);
        state.confirm_checked(&mut flash);
    }
    Ok(())
}
