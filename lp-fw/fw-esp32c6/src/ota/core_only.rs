//! Core-only: the core runs with no engine.
//!
//! What a split core does when its engine is missing, does not match, or
//! keeps crashing — and what a trial core does until it has proven itself.
//! It keeps the board reachable and nothing else: the host link up (its
//! messages drained and dropped), the watchdog fed, a trial confirmed once
//! the link is up. It sends no hello, so a host reads the board as one with
//! no usable firmware and offers a USB update, which fixes it. Taking an
//! update itself is a later milestone's.

use fw_esp32_common::usb_link::UsbLinkShared;

use super::boot_state::BootState;
use super::split_flash::SplitFlash;

/// The core-only loop. Never returns.
pub async fn core_only(
    usb_link: &'static UsbLinkShared,
    mut watchdog: crate::recovery::watchdog::WatchdogFeeder,
    mut state: BootState,
    engine_crashing: bool,
) -> ! {
    let mut flash = SplitFlash::take();
    if state.healthy {
        flash.protect(state.core_extent());
    }
    // Core-only is a complete boot for the recovery ledger — unless the core
    // is here because the engine keeps crashing, in which case the count
    // must stay up so the next boot stays here too.
    if !engine_crashing {
        lp_recovery::mark_boot_complete();
    }
    super::say!(
        "[OTA] core-only: core @{:#x} ({} B), engine room {} B",
        state.core_off,
        state.core_len,
        state.engine_extent().len()
    );
    loop {
        watchdog.feed(embassy_time::Instant::now().as_millis());
        if usb_link.is_established() {
            // A trial core's proof of life: it booted, ran its radios and got
            // a host on its link.
            state.confirm(&mut flash);
        }
        // Nothing here serves the host's messages; drain them so the link's
        // receive window keeps moving.
        while usb_link.with_link(|link| link.recv()).is_some() {}
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}
