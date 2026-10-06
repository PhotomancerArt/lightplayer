//! Core-only: the core runs with no engine.
//!
//! What a split core does when its engine is missing, is not this build's,
//! does not fit, or keeps crashing — and what a trial core does until it has
//! proven itself. It keeps the board reachable and nothing else: the host
//! link up (its messages drained and dropped), the watchdog fed, a trial
//! confirmed once the link is up. It sends no hello, so a host reads the
//! board as one with no usable firmware and offers a USB update, which fixes
//! it. Taking an update itself is a later milestone's.

use fw_esp32_common::usb_link::UsbLinkShared;

use super::boot_state::BootState;
use super::split_flash::SplitFlash;

/// Why the core is not entering an engine.
#[derive(Clone, Copy, Debug)]
pub enum CoreOnlyReason {
    /// A trial core proves itself before any engine runs.
    OnTrial,
    /// There is no engine this core may enter, and why.
    NoEngine(&'static str),
    /// The engine is there, but the last boots did not complete.
    EngineKeepsCrashing(u32),
}

/// The core-only loop. Never returns.
pub async fn core_only(
    usb_link: &'static UsbLinkShared,
    mut watchdog: crate::recovery::watchdog::WatchdogFeeder,
    mut state: BootState,
    why: CoreOnlyReason,
) -> ! {
    let mut flash = SplitFlash::take();
    if let (true, Some(layout)) = (state.trusted(), state.layout) {
        flash.protect(state.core_extent(), layout.region_end);
    }
    match why {
        CoreOnlyReason::OnTrial => {
            log::info!("[OTA] core-only: on trial, waiting for a host to confirm");
        }
        CoreOnlyReason::NoEngine(reason) => {
            log::warn!("[OTA] core-only: no engine ({reason})");
        }
        CoreOnlyReason::EngineKeepsCrashing(boots) => {
            log::error!(
                "[OTA] core-only: engine keeps crashing ({boots} incomplete boots) — not starting it"
            );
        }
    }
    // Core-only is a complete boot for the recovery ledger — unless the core
    // is here because the engine keeps crashing, in which case the count
    // must stay up so the next boot stays here too.
    if !matches!(why, CoreOnlyReason::EngineKeepsCrashing(_)) {
        lp_recovery::mark_boot_complete();
    }
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
