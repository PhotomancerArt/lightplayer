//! A [`FakeBoard`] and the [`BoardSession`] running on it, driven the way the
//! firmware drives them: effects performed, resets turned into reboots.
//!
//! - [`Effect::TrialProof`] → [`FakeBoard::confirm_trial`];
//! - [`Effect::Reset`] → the rig marks itself [`BoardRig::reset_pending`];
//!   the caller drops its links and calls [`BoardRig::reboot`];
//! - [`Effect::FlashFault`] → counted. Under a power cut the caller calls
//!   [`BoardRig::power_cycle`].

use alloc::vec::Vec;

use lpc_access::Tier;

use crate::board::{
    AccessFacts, BoardSession, Effect, LinkId, LinkTrust, Outgoing, SessionConfig, SessionMode,
};

use super::fake_board::{BootFault, FakeBoard};

/// The rig. See the module docs.
pub struct BoardRig {
    pub board: FakeBoard,
    pub session: Option<BoardSession>,
    pub access: AccessFacts,
    pub config: SessionConfig,
    /// The session asked for a reset.
    pub reset_pending: bool,
    pub flash_faults: u32,
    pub boots: u32,
}

impl BoardRig {
    /// Boot `board` and start its session.
    pub fn new(
        board: FakeBoard,
        access: AccessFacts,
        config: SessionConfig,
    ) -> Result<Self, BootFault> {
        let mut rig = Self {
            board,
            session: None,
            access,
            config,
            reset_pending: false,
            flash_faults: 0,
            boots: 0,
        };
        rig.reboot()?;
        Ok(rig)
    }

    /// Boot the board from its flash and start a fresh session.
    pub fn reboot(&mut self) -> Result<(), BootFault> {
        self.session = None;
        self.reset_pending = false;
        self.boots += 1;
        let facts = self.board.boot()?;
        let mut session =
            BoardSession::new(&mut self.board, facts, self.access.clone(), self.config);
        let effects = session.take_effects();
        self.session = Some(session);
        self.perform(effects);
        Ok(())
    }

    /// Power off and on: the flash keeps what it holds, the cut is cleared,
    /// and the board boots again.
    pub fn power_cycle(&mut self) -> Result<(), BootFault> {
        self.board.flash.power_on();
        self.reboot()
    }

    /// The session's mode, if it runs.
    #[must_use]
    pub fn mode(&self) -> Option<SessionMode> {
        self.session.as_ref().map(|s| s.facts().mode)
    }

    /// A link came up.
    pub fn link_up(&mut self, now_ms: u64, link: LinkId, trust: LinkTrust) -> Vec<Outgoing> {
        let Some(s) = &mut self.session else {
            return Vec::new();
        };
        s.link_up(now_ms, link, trust);
        self.drain()
    }

    /// A link went down.
    pub fn link_down(&mut self, now_ms: u64, link: LinkId) {
        if let Some(s) = &mut self.session {
            s.link_down(now_ms, link);
        }
    }

    /// One message from `link`. While the engine runs, `tier` is the tier
    /// the engine's own login granted that link (see
    /// [`BoardSession::on_message_with_tier`]).
    pub fn deliver(
        &mut self,
        now_ms: u64,
        link: LinkId,
        tier: Option<Tier>,
        bytes: &[u8],
    ) -> Vec<Outgoing> {
        let Some(s) = &mut self.session else {
            return Vec::new();
        };
        if s.facts().mode == SessionMode::EngineRunning {
            s.on_message_with_tier(
                &mut self.board,
                now_ms,
                link,
                LinkTrust::Untrusted,
                tier,
                bytes,
            );
        } else {
            s.on_message(&mut self.board, now_ms, link, bytes);
        }
        self.drain()
    }

    fn drain(&mut self) -> Vec<Outgoing> {
        let Some(s) = &mut self.session else {
            return Vec::new();
        };
        let out = s.take_outgoing();
        let effects = s.take_effects();
        self.perform(effects);
        out
    }

    fn perform(&mut self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::Reset => self.reset_pending = true,
                Effect::TrialProof => {
                    if self.board.confirm_trial().is_err() {
                        self.flash_faults += 1;
                    }
                }
                Effect::FlashFault => self.flash_faults += 1,
            }
        }
    }
}
