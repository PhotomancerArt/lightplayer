//! Core-only: the core runs with no engine, and takes updates.
//!
//! What a split core does when its engine is missing, is not this build's,
//! does not fit, does not hash to its digest, or keeps crashing — and what a
//! trial core does until it has proven itself. It keeps the board reachable
//! and serves the over-the-air update protocol (`lpc-update`'s board
//! session) on channel 3 of **every host link it has**: the USB link, and
//! each Bluetooth radio link when the image has them:
//!
//! - a host coming up — on any link — gets the board manifest (`M`)
//!   unprompted, and a trial core confirms itself (the split image's rule:
//!   any link coming up);
//! - an offer is checked before anything is erased, then the piece moves
//!   chunk by chunk, hashed before it is committed, and the board resets;
//! - an interrupted transfer resumes from its progress record, from any
//!   link (the link that started it owns it while it is live);
//! - an engine-less core accepts its own engine, by hash, from anyone (Y8).
//!
//! **Radio links.** Core-only is the radio port's one reader while it runs
//! (no engine, no link mux): it takes the port's `Opened`/`Closed` notices
//! and each open Bluetooth link's events itself; a LAN link (Wi-Fi) that
//! opens is asked to close, as updates over the LAN are not served yet. Every radio link is untrusted; the
//! session's own login (`L` over channel 3) is how one earns a tier, and
//! the device's `open` setting counts as the access rule says (QY2). The
//! links were opened in update mode (`RadioLinkMode::Update`, decided by
//! `split_boot` before the radio side may open any): they advertise the
//! wide receive window, so a host can keep a window of chunks in flight.
//! There is no login deadline here — a radio link can only query, heal, or
//! log in — so the BLE task's own subscribe deadline is the only one.
//!
//! It sends no hello on any link: a host that only reads channel 1 still
//! sees a board with no usable firmware and offers a USB update, which also
//! fixes it.

#[cfg(feature = "ble")]
use alloc::vec::Vec;

use fw_esp32_common::usb_link::UsbLinkShared;
use lpc_update::board::{
    AccessFacts, EngineStatus, LinkTrust, OWNER_QUIET_MS, SessionConfig, SessionMode,
};
use lpc_wire::lp_link::{CH_UPDATE, LinkEvent};

use super::board_identity::{CoreIdentity, board_facts};
use super::boot_state::BootState;
use super::status_light::StatusLight;
use super::update_edge::{EdgeEffect, UpdateEdge, state_word};
use super::update_links::{USB_LINK, UpdateLinks};
use super::update_target_impl::SplitUpdateTarget;

/// How long a committed piece's last log lines get to reach the link before
/// the reset is asked for.
const LOG_GRACE: embassy_time::Duration = embassy_time::Duration::from_millis(100);
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
    /// The radio links' port, already in update mode.
    #[cfg(feature = "ble")]
    pub radio_port: &'static fw_esp32_common::radio_link::RadioLinkPort,
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
    /// The update light, when the engine left a record this core can light.
    pub light: Option<StatusLight>,
}

/// The core-only loop. Never returns: every committed piece ends in a reset.
pub async fn core_only(ctx: CoreOnly) -> ! {
    let CoreOnly {
        usb_link,
        #[cfg(feature = "ble")]
        radio_port,
        mut watchdog,
        mut state,
        why,
        identity,
        access,
        usb_trust,
        entropy,
        mut light,
    } = ctx;
    let links = UpdateLinks {
        usb: usb_link,
        #[cfg(feature = "ble")]
        radio: radio_port,
    };
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
        // Encoding 1: a `Z` chunk decodes with lp-deflate on this task, in a
        // 32 KiB + 4 KiB window allocated at the first `Z` (DM18: core-only
        // has the stack and the heap, the engine not running).
        takes_encoding_1: true,
        entropy: Some(entropy),
        owner_quiet_ms: OWNER_QUIET_MS,
    };
    let mut edge = UpdateEdge::new(SplitUpdateTarget::new(&state), facts, access, config);
    let mut shown = edge.state();
    log::info!("[OTA] core-only: {}", state_word(shown));
    if let Some(light) = light.as_mut() {
        light.show(shown);
    }
    log::info!("[OTA] core-only heap free {} B", esp_alloc::HEAP.free());
    // A host whose link came up before this loop started: its `Up` may
    // already be gone, so the link's state says it.
    if usb_link.is_established() {
        edge.link_up(USB_LINK, usb_trust);
    }
    #[cfg(feature = "ble")]
    let mut radio = RadioLinks::new(radio_port);

    let mut reset_at: Option<embassy_time::Instant> = None;
    loop {
        watchdog.feed(embassy_time::Instant::now().as_millis());
        let mut touched = false;
        while let Some(event) = usb_link.with_link(|link| link.recv()) {
            touched = true;
            match event {
                LinkEvent::Up { .. } => edge.link_up(USB_LINK, usb_trust),
                LinkEvent::Reset { .. } => edge.link_down(USB_LINK),
                LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                    edge.on_message(USB_LINK, &data);
                }
                // Channel 1 (the wire) has no server to answer it here.
                _ => {}
            }
        }
        #[cfg(feature = "ble")]
        {
            touched |= radio.pump(&mut edge);
        }
        for effect in edge.pump(&links) {
            touched = true;
            match effect {
                EdgeEffect::ConfirmTrial => state.confirm(edge.target.flash()),
                EdgeEffect::Reset => {
                    // What the piece cost: the main stack's high-water mark
                    // (inflate's frames included, DM18) and the heap left
                    // with the `Z` window still held.
                    crate::stack_probe::log_if_grown("core-only");
                    log::info!(
                        "[OTA] core-only heap free {} B at commit ({} chunks arrived as Z)",
                        esp_alloc::HEAP.free(),
                        edge.encoded_chunks()
                    );
                    reset_at = Some(embassy_time::Instant::now() + LOG_GRACE);
                }
            }
        }
        // The log lines above ride the link's best-effort log channel: give
        // them a moment to reach it, then the link task resets once the
        // host has everything (or after its drain limit).
        if reset_at.is_some_and(|at| embassy_time::Instant::now() >= at) {
            reset_at = None;
            fw_esp32_common::usb_link::when_drained(super::reset_now);
        }
        if touched {
            let now = edge.state();
            if now != shown {
                shown = now;
                log::info!("[OTA] core-only: {}", state_word(now));
                if let Some(light) = light.as_mut() {
                    light.show(now);
                }
            }
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}

/// The radio links core-only serves: the ones the port announced open, and
/// the slot each is on.
#[cfg(feature = "ble")]
struct RadioLinks {
    port: &'static fw_esp32_common::radio_link::RadioLinkPort,
    open: Vec<(lpc_shared::transport::LinkId, usize)>,
}

#[cfg(feature = "ble")]
impl RadioLinks {
    fn new(port: &'static fw_esp32_common::radio_link::RadioLinkPort) -> Self {
        Self {
            port,
            open: Vec::new(),
        }
    }

    /// Take the port's notices and every open link's events into the
    /// session. Whether anything happened.
    fn pump(&mut self, edge: &mut UpdateEdge) -> bool {
        use fw_esp32_common::radio_link::RadioLinkEvent;

        use super::update_links::session_link;

        let mut touched = false;
        while let Some(event) = self.port.try_event() {
            touched = true;
            match event {
                // A LAN link (Wi-Fi) is not served here: updates over the
                // LAN are their own change, and its secure handshake needs
                // the server's keys, which core-only does not run.
                RadioLinkEvent::Opened { link, slot }
                    if slot >= fw_esp32_common::radio_link::RADIO_LINK_SLOTS =>
                {
                    log::info!("[OTA] core-only: LAN link {link} refused (slot {slot})");
                    self.port
                        .slot(slot)
                        .request_close("core-only serves Bluetooth links only");
                }
                RadioLinkEvent::Opened { link, slot } => {
                    log::info!("[OTA] core-only: radio link {link} opened (slot {slot})");
                    self.open.push((link, slot));
                }
                RadioLinkEvent::Closed { link } => {
                    self.open.retain(|(l, _)| *l != link);
                    edge.link_down(session_link(link));
                    log::info!("[OTA] core-only: radio link {link} closed");
                }
            }
        }
        for &(link, slot) in &self.open {
            let id = session_link(link);
            while let Some(event) = self.port.slot(slot).recv(link) {
                touched = true;
                match event {
                    // Every radio link is untrusted: the session's login
                    // gives it a tier.
                    LinkEvent::Up { .. } => edge.link_up(id, LinkTrust::Untrusted),
                    LinkEvent::Reset { .. } => edge.link_down(id),
                    LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                        edge.on_message(id, &data);
                    }
                    // Channel 1 (the wire) has no server to answer it here.
                    _ => {}
                }
            }
        }
        touched
    }
}
