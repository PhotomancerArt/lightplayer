//! The board's update session (`lpc_update::board::BoardSession`) on the
//! host links — USB, and each radio link when the image has Bluetooth: the
//! edge both core-only and the running engine's hooks drive. It owns the
//! session, the split image's [`SplitUpdateTarget`] and one channel-3 outbox
//! per link it answers ([`UpdateOutbox`], sent through [`UpdateLinks`]),
//! reads the clock for the session, and says what happened in one stable
//! `[OTA]` line per state change (the emulator scenarios read them):
//!
//! - `[OTA] offer <buildId> → <core|engine> @<dest>`
//! - `[OTA] resuming <core|engine> at <bytes>`
//! - `[OTA] <core|engine> verified, committing`
//! - `[OTA] refused <reason>`
//! - `[OTA] resetting: <why>`

use alloc::vec::Vec;

use lpc_update::board::{
    AccessFacts, BoardFacts, BoardSession, Effect, LinkId, LinkTrust, SessionConfig, SessionMode,
    TransferProgress,
};
use lpc_update::{BoardMessage, BoardState, HostMessage, PieceKind};

use super::update_links::UpdateLinks;
use super::update_outbox::UpdateOutbox;
use super::update_target_impl::SplitUpdateTarget;
use super::update_timing::{MessageTiming, now_us};

/// What the firmware does after a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeEffect {
    /// A link came up on a trial core: confirm it (the boot record is the
    /// firmware's).
    ConfirmTrial,
    /// Reset the chip.
    Reset,
}

/// The session, its flash and its outboxes.
pub struct UpdateEdge {
    pub session: BoardSession,
    pub target: SplitUpdateTarget,
    /// One per link the session has answered, while its link is open or it
    /// still holds something.
    outboxes: Vec<UpdateOutbox>,
    /// The last transfer the session ran, for the commit line.
    last: Option<TransferProgress>,
    /// The last message was answered `N`.
    refused_last: bool,
    /// `Z` chunks received (the board's own count; a re-requested chunk
    /// counts each time).
    encoded: u32,
    /// The chunks (`D`/`Z`) and read-backs (`G`) this session handled, for
    /// the `[OTA] timing` lines.
    chunks: MessageTiming,
    read_backs: MessageTiming,
}

/// Milliseconds since boot: the session's clock.
pub fn now_ms() -> u64 {
    embassy_time::Instant::now().as_millis()
}

fn kind_word(kind: PieceKind) -> &'static str {
    match kind {
        PieceKind::Core => "core",
        PieceKind::Engine => "engine",
    }
}

/// A board state as the manifest spells it.
pub fn state_word(state: BoardState) -> &'static str {
    match state {
        BoardState::Running => "running",
        BoardState::NeedsEngine => "needs-engine",
        BoardState::EngineCrashing => "engine-crashing",
        BoardState::Updating => "updating",
        BoardState::OnTrial => "on-trial",
        BoardState::Unknown => "unknown",
    }
}

impl UpdateEdge {
    pub fn new(
        mut target: SplitUpdateTarget,
        facts: BoardFacts,
        access: AccessFacts,
        config: SessionConfig,
    ) -> Self {
        let session = BoardSession::new(&mut target, facts, access, config);
        let last = session.transfer_progress();
        Self {
            session,
            target,
            outboxes: Vec::new(),
            last,
            refused_last: false,
            encoded: 0,
            chunks: MessageTiming::default(),
            read_backs: MessageTiming::default(),
        }
    }

    /// The board's state, as its manifest says it now.
    pub fn state(&self) -> BoardState {
        self.session.manifest(now_ms()).state
    }

    /// `link`'s session came up, trusted as `trust` (USB: trusted; a radio
    /// link: untrusted — a core-side login gives it a tier).
    pub fn link_up(&mut self, link: LinkId, trust: LinkTrust) {
        self.session.link_up(now_ms(), link, trust);
    }

    /// `link`'s session ended, or the link is gone: the session forgets it
    /// (a login challenge it held, a transfer it owned may be taken over),
    /// and so does its outbox.
    pub fn link_down(&mut self, link: LinkId) {
        self.session.link_down(now_ms(), link);
        self.outboxes.retain(|o| o.link != link);
    }

    /// `Z` chunks this session received.
    pub fn encoded_chunks(&self) -> u32 {
        self.encoded
    }

    /// One channel-3 message from `link`, trusted as it came up (in
    /// core-only the session's own login gives an untrusted link its tier).
    pub fn on_message(&mut self, link: LinkId, bytes: &[u8]) {
        let first = bytes.first().copied();
        if first == Some(b'Z') {
            self.encoded += 1;
        }
        let start = now_us();
        self.session
            .on_message(&mut self.target, now_ms(), link, bytes);
        self.after_message(first, start, bytes);
    }

    /// One channel-3 message from a radio `link` while the engine runs, with
    /// the tier a login or key granted it on the engine's server (never one
    /// the device's `open` setting alone gave: the session adds that itself,
    /// except through the relay, where it never applies).
    #[cfg(feature = "ble")]
    pub fn on_message_with_tier(
        &mut self,
        link: LinkId,
        relayed: bool,
        granted: Option<lpc_access::Tier>,
        bytes: &[u8],
    ) {
        let first = bytes.first().copied();
        let start = now_us();
        let trust = if relayed {
            LinkTrust::Relayed(None)
        } else {
            LinkTrust::Untrusted
        };
        self.session
            .on_message_with_tier(&mut self.target, now_ms(), link, trust, granted, bytes);
        self.after_message(first, start, bytes);
    }

    fn after_message(&mut self, first: Option<u8>, start: u64, bytes: &[u8]) {
        match first {
            Some(b'D' | b'Z') => self.chunks.note(start, now_us()),
            Some(b'G') => self.read_backs.note(start, now_us()),
            _ => {}
        }
        self.refused_last = self.session_refused();
        self.note_offer(bytes);
    }

    /// Send what the session queued, each answer on the link it is for, and
    /// take its effects.
    pub fn pump(&mut self, links: &UpdateLinks) -> Vec<EdgeEffect> {
        for out in self.session.take_outgoing() {
            if let Ok(BoardMessage::Refusal(r)) = BoardMessage::decode(&out.bytes) {
                // In words, never `{:?}`: this image prints a Debug as
                // nothing (`-Z fmt-debug=none`), which left the line empty.
                log::warn!("[OTA] refused on link {}: {r}", out.link.0);
            }
            self.outbox_for(out.link).push(links, out.bytes);
        }
        for outbox in &mut self.outboxes {
            outbox.flush(links);
        }
        // A link that is gone, with nothing waiting for it, needs no outbox.
        self.outboxes
            .retain(|o| !o.is_empty() || links.generation(o.link).is_some());
        if let Some(now) = self.session.transfer_progress() {
            self.last = Some(now);
        }
        let mut effects = Vec::new();
        for effect in self.session.take_effects() {
            match effect {
                Effect::TrialProof => effects.push(EdgeEffect::ConfirmTrial),
                Effect::FlashFault => log::error!(
                    "[OTA] a flash operation failed; the transfer stops where its record says"
                ),
                Effect::Reset => {
                    effects.push(EdgeEffect::Reset);
                    self.note_reset();
                }
            }
        }
        effects
    }

    fn outbox_for(&mut self, link: LinkId) -> &mut UpdateOutbox {
        let at = match self.outboxes.iter().position(|o| o.link == link) {
            Some(at) => at,
            None => {
                self.outboxes.push(UpdateOutbox::new(link));
                self.outboxes.len() - 1
            }
        };
        &mut self.outboxes[at]
    }

    /// Whether the session's newest answer (not yet sent) is a refusal.
    fn session_refused(&self) -> bool {
        self.session
            .peek_outgoing()
            .last()
            .is_some_and(|o| o.bytes.first() == Some(&b'N'))
    }

    fn note_offer(&mut self, bytes: &[u8]) {
        let Ok(HostMessage::Offer(offer)) = HostMessage::decode(bytes) else {
            return;
        };
        let id = core::str::from_utf8(offer.build_id_text()).unwrap_or("?");
        match self.session.transfer_progress() {
            // Accepted: a new transfer, or the one in flight (resumed or
            // taken over). A refused offer says so in `pump`.
            Some(t) if !self.refused_last => {
                log::info!("[OTA] offer {id} → {} @{:#x}", kind_word(t.kind), t.dest);
                if t.done > 0 {
                    log::info!("[OTA] resuming {} at {}", kind_word(t.kind), t.done);
                }
                self.last = Some(t);
            }
            Some(_) => {}
            None if self.session.halted()
                && self.session.facts().mode == SessionMode::EngineRunning =>
            {
                log::info!("[OTA] offer {id} → core (pending): handing over to core-only");
            }
            None => {}
        }
    }

    fn note_reset(&mut self) {
        let flash = self.target.timing;
        self.read_backs.log("read-back", &flash);
        let what = match self.last {
            Some(t) => kind_word(t.kind),
            None => "chunks",
        };
        self.chunks.log(what, &flash);
        if self.session.facts().mode == SessionMode::EngineRunning {
            log::info!("[OTA] resetting: a core install is pending");
            return;
        }
        match self.last.take() {
            Some(t) => {
                log::info!("[OTA] {} verified, committing", kind_word(t.kind));
                log::info!("[OTA] resetting: the {} is in", kind_word(t.kind));
            }
            None => log::info!("[OTA] resetting"),
        }
    }
}

/// Core-only's radio and LAN links drive the session through the edge
/// (`fw_esp32_common::radio_link::CoreOnlyLinks`): a LAN link's key lookup
/// is answered by the session itself, on the session's clock.
#[cfg(feature = "ble")]
impl fw_esp32_common::radio_link::CoreOnlySession for UpdateEdge {
    fn link_up(&mut self, link: LinkId, trust: LinkTrust) {
        UpdateEdge::link_up(self, link, trust);
    }

    fn link_down(&mut self, link: LinkId) {
        UpdateEdge::link_down(self, link);
    }

    fn on_message(&mut self, link: LinkId, bytes: &[u8]) {
        UpdateEdge::on_message(self, link, bytes);
    }

    #[cfg(feature = "wifi")]
    fn key_lookup(
        &mut self,
        link: LinkId,
        path: lpc_update::board::NetworkPath,
        salt: &[u8; 16],
    ) -> lpc_update::board::CoreKeyAnswer {
        self.session.key_lookup(now_ms(), link, path, salt)
    }

    #[cfg(feature = "wifi")]
    fn key_wrong(&mut self, link: LinkId) {
        self.session.key_wrong(now_ms(), link);
    }

    #[cfg(feature = "wifi")]
    fn key_authenticated(
        &mut self,
        link: LinkId,
        path: lpc_update::board::NetworkPath,
        candidate: u8,
    ) -> LinkTrust {
        self.session.key_authenticated(link, path, candidate)
    }
}
