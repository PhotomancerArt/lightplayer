//! The board's update session (`lpc_update::board::BoardSession`) on the USB
//! link: the edge both core-only and the running engine's hook drive. It
//! owns the session, the split image's [`SplitUpdateTarget`] and the
//! channel-3 outbox, reads the clock for the session, and says what happened
//! in one stable `[OTA]` line per state change (the emulator scenarios read
//! them):
//!
//! - `[OTA] offer <buildId> → <core|engine> @<dest>`
//! - `[OTA] resuming <core|engine> at <bytes>`
//! - `[OTA] <core|engine> verified, committing`
//! - `[OTA] refused <reason>`
//! - `[OTA] resetting: <why>`

use alloc::vec::Vec;

use fw_esp32_common::usb_link::UsbLinkShared;
use lpc_update::board::{
    AccessFacts, BoardFacts, BoardSession, Effect, LinkId, LinkTrust, SessionConfig, SessionMode,
    TransferProgress,
};
use lpc_update::{BoardMessage, BoardState, HostMessage, PieceKind};

use super::update_outbox::UpdateOutbox;
use super::update_target_impl::SplitUpdateTarget;

/// The one USB link, as the session names it.
pub const USB_LINK: LinkId = LinkId(0);

/// What the firmware does after a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeEffect {
    /// A link came up on a trial core: confirm it (the boot record is the
    /// firmware's).
    ConfirmTrial,
    /// Reset the chip.
    Reset,
}

/// The session, its flash and its outbox.
pub struct UpdateEdge {
    pub session: BoardSession,
    pub target: SplitUpdateTarget,
    outbox: UpdateOutbox,
    /// The last transfer the session ran, for the commit line.
    last: Option<TransferProgress>,
    /// The last message was answered `N`.
    refused_last: bool,
    /// `Z` chunks received (the board's own count; a re-requested chunk
    /// counts each time).
    encoded: u32,
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
            outbox: UpdateOutbox::new(),
            last,
            refused_last: false,
            encoded: 0,
        }
    }

    /// The board's state, as its manifest says it now.
    pub fn state(&self) -> BoardState {
        self.session.manifest(now_ms()).state
    }

    pub fn link_up(&mut self, trust: LinkTrust) {
        self.session.link_up(now_ms(), USB_LINK, trust);
    }

    pub fn link_down(&mut self) {
        self.session.link_down(now_ms(), USB_LINK);
    }

    /// `Z` chunks this session received.
    pub fn encoded_chunks(&self) -> u32 {
        self.encoded
    }

    /// One channel-3 message from the USB host.
    pub fn on_message(&mut self, bytes: &[u8]) {
        if bytes.first() == Some(&b'Z') {
            self.encoded += 1;
        }
        self.session
            .on_message(&mut self.target, now_ms(), USB_LINK, bytes);
        self.refused_last = self.session_refused();
        self.note_offer(bytes);
    }

    /// Send what the session queued and take its effects.
    pub fn pump(&mut self, link: &UsbLinkShared) -> Vec<EdgeEffect> {
        for out in self.session.take_outgoing() {
            if let Ok(BoardMessage::Refusal(r)) = BoardMessage::decode(&out.bytes) {
                log::warn!("[OTA] refused {r:?}");
            }
            self.outbox.push(link, out.bytes);
        }
        self.outbox.flush(link);
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
