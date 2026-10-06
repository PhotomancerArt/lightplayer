//! Core-only: the core runs with no engine, and takes updates.
//!
//! What a split core does when its engine is missing, is not this build's,
//! does not fit, does not hash to its digest, or keeps crashing — and what a
//! trial core does until it has proven itself. It keeps the board reachable
//! and serves the over-the-air update protocol (`lpc-update`'s board
//! session) on the USB link's channel 3:
//!
//! - a host coming up gets the board manifest (`M`) unprompted, and a trial
//!   core confirms itself (the split image's rule: any link coming up);
//! - an offer is checked before anything is erased, then the piece moves
//!   chunk by chunk, hashed before it is committed, and the board resets;
//! - an interrupted transfer resumes from its progress record;
//! - an engine-less core accepts its own engine, by hash, from anyone (Y8).
//!
//! It sends no hello: a host that only reads channel 1 still sees a board
//! with no usable firmware and offers a USB update, which also fixes it.

use fw_esp32_common::usb_link::UsbLinkShared;
use lpc_update::board::{
    AccessFacts, EngineStatus, LinkTrust, OWNER_QUIET_MS, SessionConfig, SessionMode,
};
use lpc_wire::lp_link::{CH_UPDATE, LinkEvent};

use super::board_identity::{CoreIdentity, board_facts};
use super::boot_state::BootState;
use super::update_edge::{EdgeEffect, UpdateEdge, state_word};
use super::update_target_impl::SplitUpdateTarget;

/// Why the core is not entering an engine.
#[derive(Clone, Copy, Debug)]
pub enum CoreOnlyReason {
    /// A trial core proves itself before any engine runs.
    OnTrial,
    /// There is no engine this core may enter, and why.
    NoEngine(&'static str),
    /// The engine is there, but the last boots did not complete. Its
    /// length, from its valid header.
    EngineKeepsCrashing { boots: u32, engine_len: u32 },
}

/// Everything core-only needs from the boot.
pub struct CoreOnly {
    pub usb_link: &'static UsbLinkShared,
    pub watchdog: crate::recovery::watchdog::WatchdogFeeder,
    pub state: BootState,
    pub why: CoreOnlyReason,
    pub identity: CoreIdentity,
    /// `/.lp/access.json` as the core reads it (`secrets` and `open` only).
    pub access: AccessFacts,
    /// How the USB link is trusted: always, unless a test fixture says not.
    pub usb_trust: LinkTrust,
    /// The chip's RNG, for login nonces.
    pub entropy: fn(&mut [u8]),
}

/// The core-only loop. Never returns: every committed piece ends in a reset.
pub async fn core_only(ctx: CoreOnly) -> ! {
    let CoreOnly {
        usb_link,
        mut watchdog,
        mut state,
        why,
        identity,
        access,
        usb_trust,
        entropy,
    } = ctx;
    let (engine, engine_len) = match why {
        CoreOnlyReason::OnTrial => {
            log::info!("[OTA] core-only: on trial, waiting for a host to confirm");
            (EngineStatus::Missing, None)
        }
        CoreOnlyReason::NoEngine(reason) => {
            log::warn!("[OTA] core-only: no engine ({reason})");
            (EngineStatus::Missing, None)
        }
        CoreOnlyReason::EngineKeepsCrashing { boots, engine_len } => {
            log::error!(
                "[OTA] core-only: engine keeps crashing ({boots} incomplete boots) — not starting it"
            );
            (EngineStatus::Crashing, Some(engine_len))
        }
    };
    // Core-only is a complete boot for the recovery ledger — unless the core
    // is here because the engine keeps crashing, in which case the count
    // must stay up so the next boot stays here too.
    if !matches!(why, CoreOnlyReason::EngineKeepsCrashing { .. }) {
        lp_recovery::mark_boot_complete();
    }

    let facts = board_facts(&state, &identity, SessionMode::CoreOnly, engine, engine_len);
    let config = SessionConfig {
        takes_encoding_1: false,
        entropy: Some(entropy),
        owner_quiet_ms: OWNER_QUIET_MS,
    };
    let mut edge = UpdateEdge::new(SplitUpdateTarget::new(&state), facts, access, config);
    let mut shown = edge.state();
    log::info!("[OTA] core-only: {}", state_word(shown));
    // A host whose link came up before this loop started: its `Up` may
    // already be gone, so the link's state says it.
    if usb_link.is_established() {
        edge.link_up(usb_trust);
    }

    loop {
        watchdog.feed(embassy_time::Instant::now().as_millis());
        let mut touched = false;
        while let Some(event) = usb_link.with_link(|link| link.recv()) {
            touched = true;
            match event {
                LinkEvent::Up { .. } => edge.link_up(usb_trust),
                LinkEvent::Reset { .. } => edge.link_down(),
                LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                    edge.on_message(&data);
                }
                // Channel 1 (the wire) has no server to answer it here.
                _ => {}
            }
        }
        for effect in edge.pump(usb_link) {
            touched = true;
            match effect {
                EdgeEffect::ConfirmTrial => state.confirm(edge.target.flash()),
                EdgeEffect::Reset => {
                    // The link task resets once the host has everything
                    // (or after its drain limit).
                    fw_esp32_common::usb_link::when_drained(super::reset_now);
                }
            }
        }
        if touched {
            let now = edge.state();
            if now != shown {
                shown = now;
                log::info!("[OTA] core-only: {}", state_word(now));
            }
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}
