//! **The board's update session**: a sans-IO state machine the firmware
//! drives with link events and channel-3 messages, and which answers with
//! messages to send and effects to perform.
//!
//! ```text
//! firmware ──link_up / link_down / on_message(target, now_ms, link, bytes)──▶ BoardSession
//!          ◀────────────── take_outgoing() / take_effects() ────────────────
//!          ◀── UpdateTarget calls: erase / program / read / format hooks
//! ```
//!
//! It runs in one of two modes ([`SessionMode`]): **core-only** (no engine
//! runs; the session serves every link and moves pieces) and **engine
//! running** (the session is the engine's channel-3 hook: `Q`, `G`, and an
//! offer that hands over to core-only, [`super::running_engine`]).
//!
//! # The offer
//!
//! An offer is checked **before anything is erased** (DM10, E8), in this
//! order, the first failure answering:
//!
//! 1. a must-understand flag this session does not know → `N`/`V` (`what` 4);
//! 2. the boot state is untrusted → `N`/`T`;
//! 3. chip, layout (equal), loader (≥ `min_loader`) → `N`/`V`;
//! 4. the install kind, by hashes ([`crate::install_kind`]); an offer that
//!    contradicts itself → `N`/`H`;
//! 5. access for that kind ([`super::access_rule`]) → `N`/`A`;
//! 6. any install on a trial core no link has come up on yet → `N`/`T`: a
//!    trial confirms before it fetches (#903's rule), because its engine
//!    goes where the previous core still is, and an unconfirmed trial may
//!    yet roll back to it. Once a link comes up ([`Effect::TrialProof`]) it
//!    may fetch its own engine; it never takes another core on trial;
//! 7. a core install of the build that failed its trial here → `N`/`F`;
//! 8. fit (core and engine, [`UpdateTarget::core_dest`],
//!    [`UpdateTarget::engine_room_for`]) → `N`/`S`;
//! 9. another link owns a transfer and is live → `N`/`B`
//!    ([`super::transfer_owner`]).
//!
//! Then the piece moves ([`super::piece_stage`]). A host message type the
//! session does not know is answered `N`/`U` with its type byte and changes
//! nothing.
//!
//! # Resume (DM12)
//!
//! At start the session reads the progress record and applies the
//! foreign-record rule: its own transfer is restored at the first unwritten
//! chunk, and the next matching offer, from any link, resumes it without
//! erasing. A foreign record is ignored and erased when the next transfer
//! starts. The running engine instead clears any record it finds: a valid
//! engine wins (DM13).

use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use lpc_access::{LoginState, Tier};

use crate::code_table::CHUNK;
use crate::flag_rule::{OFFER_FLAGS_KNOWN_V1, unknown_must_understand};
use crate::install_kind::{InstallKind, install_kind};
use crate::message::HostMessage;
use crate::offer::Offer;
use crate::piece_kind::PieceKind;
use crate::refusal::{Mismatch, Refusal};
use crate::transfer_record::{OwnFacts, RecordClass, RecordRead, RecordStage, TransferRecord};

use super::access_rule::{AccessFacts, Operation, may};
use super::board_link::{BoardLink, LinkId, LinkTrust};
use super::session_output::{Effect, Outgoing, SessionConfig};
use super::transfer::Transfer;
use super::transfer_owner::owner_live;
use super::update_target::{BoardFacts, EngineStatus, SessionMode, UpdateTarget};
use super::update_window::UpdateWindow;

/// A transfer as [`BoardSession::transfer_progress`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferProgress {
    pub kind: PieceKind,
    /// The flash address of the piece's first byte.
    pub dest: u32,
    /// Bytes written and read back.
    pub done: u32,
    pub total: u32,
}

/// The board's update session. See the module docs.
pub struct BoardSession {
    pub(super) facts: BoardFacts,
    pub(super) access: AccessFacts,
    pub(super) config: SessionConfig,
    /// Where this core's engine may live.
    pub(super) own_engine_room: Range<u32>,
    pub(super) links: Vec<BoardLink>,
    pub(super) transfer: Option<Transfer>,
    pub(super) login: LoginState,
    /// The link that holds the device's one login challenge.
    pub(super) login_link: Option<LinkId>,
    /// Secure links whose key lookup was answered: each candidate's tier
    /// (`None`: the anonymous key), until the link comes up
    /// ([`super::core_key_lookup`]).
    pub(super) key_lookups: Vec<(LinkId, Vec<Option<Tier>>)>,
    pub(super) window: Option<UpdateWindow>,
    /// One sector of scratch, allocated once.
    pub(super) sector_buf: Vec<u8>,
    /// The engine header is valid right now: read-back is served, and the
    /// manifest reports `engineLen`. Cleared when the session erases it.
    pub(super) engine_valid: bool,
    /// An unconfirmed trial core, until a link comes up.
    pub(super) on_trial: bool,
    outbox: Vec<Outgoing>,
    effects: Vec<Effect>,
    /// A reset was asked for: the chip is about to go, so nothing more is
    /// done.
    halted: bool,
}

impl BoardSession {
    /// Start a session. Core-only reads the progress record and keeps this
    /// core's own transfer, if any; the running engine clears a record it
    /// finds (a valid engine wins, DM13).
    pub fn new<T: UpdateTarget>(
        target: &mut T,
        facts: BoardFacts,
        access: AccessFacts,
        config: SessionConfig,
    ) -> Self {
        let own_engine_room = target.engine_room_for(facts.core_off, facts.core_len);
        let engine_valid = facts.mode == SessionMode::EngineRunning
            || matches!(facts.engine, EngineStatus::Valid | EngineStatus::Crashing);
        let mut session = Self {
            on_trial: facts.on_trial,
            facts,
            access,
            config,
            own_engine_room,
            links: Vec::new(),
            transfer: None,
            login: LoginState::new(),
            login_link: None,
            key_lookups: Vec::new(),
            window: None,
            sector_buf: vec![0u8; CHUNK as usize],
            outbox: Vec::new(),
            effects: Vec::new(),
            engine_valid,
            halted: false,
        };
        session.restore(target);
        session
    }

    fn restore<T: UpdateTarget>(&mut self, target: &mut T) {
        let addr = self.facts.progress_record_addr;
        if target.read(addr, &mut self.sector_buf).is_err() {
            self.effects.push(Effect::FlashFault);
            return;
        }
        let read = TransferRecord::read(&self.sector_buf);
        match self.facts.mode {
            SessionMode::EngineRunning => {
                if !matches!(read, RecordRead::Nothing) && target.erase_sector(addr).is_err() {
                    self.effects.push(Effect::FlashFault);
                }
            }
            SessionMode::CoreOnly => {
                if let RecordRead::V1(record, marks) = read {
                    let own = OwnFacts {
                        build_hash: self.facts.build_hash(),
                        engine_start: self.own_engine_room.start,
                        refused_build: self.facts.refused_build,
                    };
                    if record.classify(&own, |len| target.core_dest(len)) == RecordClass::Own {
                        self.transfer = Some(Transfer::new(record, marks));
                    }
                }
            }
        }
    }

    // ---- Inputs ------------------------------------------------------------

    /// A link came up with `trust`. In core-only it is sent `M` unprompted,
    /// and an unconfirmed trial core reports [`Effect::TrialProof`] (the
    /// split image's rule: any link coming up confirms a trial; the boot
    /// record is the firmware's, so the session only reports it).
    pub fn link_up(&mut self, now_ms: u64, link: LinkId, trust: LinkTrust) {
        self.links.retain(|l| l.id != link);
        self.links.push(BoardLink {
            id: link,
            trust,
            granted: None,
            last_rx_ms: now_ms,
        });
        if self.halted || self.facts.mode != SessionMode::CoreOnly {
            return;
        }
        self.send_manifest(now_ms, link);
        if self.on_trial {
            self.on_trial = false;
            self.effects.push(Effect::TrialProof);
        }
    }

    /// A link went away: the login challenge it held is freed, its granted
    /// tier is forgotten, and a transfer it owned may be taken over.
    pub fn link_down(&mut self, _now_ms: u64, link: LinkId) {
        self.links.retain(|l| l.id != link);
        self.key_lookups.retain(|(l, _)| *l != link);
        if self.login_link == Some(link) {
            self.login_link = None;
            self.login.cancel();
        }
    }

    /// One channel-3 message from `link`. A link the session has not heard
    /// come up is taken as [`LinkTrust::Untrusted`]. A message that does not
    /// decode is dropped.
    pub fn on_message<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        now_ms: u64,
        link: LinkId,
        bytes: &[u8],
    ) {
        if self.halted {
            return;
        }
        match self.links.iter_mut().find(|l| l.id == link) {
            Some(l) => l.last_rx_ms = now_ms,
            None => self.links.push(BoardLink {
                id: link,
                trust: LinkTrust::Untrusted,
                granted: None,
                last_rx_ms: now_ms,
            }),
        }
        let Ok(msg) = HostMessage::decode(bytes) else {
            return;
        };
        let core_only = self.facts.mode == SessionMode::CoreOnly;
        match msg {
            HostMessage::Query { .. } => self.send_manifest(now_ms, link),
            HostMessage::Offer(offer) => self.on_offer(target, now_ms, link, &offer),
            HostMessage::Chunk(chunk) if core_only => self.on_chunk(target, link, &chunk),
            HostMessage::ReadBack(g) => self.on_read_back(target, link, &g),
            HostMessage::Login(step) if core_only => self.on_login(now_ms, link, step),
            HostMessage::Unknown { ty } => self.refuse(link, Refusal::UnknownMessage { ty }),
            // Known messages that do not apply while the engine runs.
            HostMessage::Chunk(_) | HostMessage::Login(_) => {}
        }
    }

    /// [`on_message`](Self::on_message) for the running engine: the server's
    /// own login holds the link's tier, and the firmware passes the tier a
    /// **login or a key** granted (never one `OpenTo` alone gave: the
    /// session adds `OpenTo` itself, so QY2's switch applies the same way in
    /// both modes). `trust` is how a link the session has not heard yet is
    /// taken: [`LinkTrust::Untrusted`] nearby, `LinkTrust::Relayed(None)`
    /// through the relay (where `OpenTo` never applies).
    pub fn on_message_with_tier<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        now_ms: u64,
        link: LinkId,
        trust: LinkTrust,
        tier: Option<Tier>,
        bytes: &[u8],
    ) {
        if !self.links.iter().any(|l| l.id == link) {
            self.link_up(now_ms, link, trust);
        }
        if let Some(l) = self.links.iter_mut().find(|l| l.id == link) {
            l.granted = tier;
        }
        self.on_message(target, now_ms, link, bytes);
    }

    // ---- Outputs -----------------------------------------------------------

    /// Messages to send, in order.
    pub fn take_outgoing(&mut self) -> Vec<Outgoing> {
        core::mem::take(&mut self.outbox)
    }

    /// Messages queued and not yet taken, oldest first.
    #[must_use]
    pub fn peek_outgoing(&self) -> &[Outgoing] {
        &self.outbox
    }

    /// Effects to perform, in order.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        core::mem::take(&mut self.effects)
    }

    /// The facts the session started with.
    #[must_use]
    pub fn facts(&self) -> &BoardFacts {
        &self.facts
    }

    /// Whether a transfer is pending or running.
    #[must_use]
    pub fn transferring(&self) -> bool {
        self.transfer.is_some()
    }

    /// Whether the session asked for a reset and stopped.
    #[must_use]
    pub fn halted(&self) -> bool {
        self.halted
    }

    /// The transfer pending or running, for the firmware's log lines and
    /// light: its piece, where it goes, and how far it got.
    #[must_use]
    pub fn transfer_progress(&self) -> Option<TransferProgress> {
        self.transfer.as_ref().map(|t| TransferProgress {
            kind: t.kind(),
            dest: t.record.dest,
            done: t.done_bytes(),
            total: t.record.len,
        })
    }

    // ---- The offer -----------------------------------------------------------

    fn on_offer<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        now_ms: u64,
        link: LinkId,
        offer: &Offer,
    ) {
        let kind = match self.check_offer(target, link, offer) {
            Ok(kind) => kind,
            Err(refusal) => return self.refuse(link, refusal),
        };
        let record = match self.fit(target, offer, kind) {
            Ok(record) => record,
            Err(refusal) => return self.refuse(link, refusal),
        };
        match self.facts.mode {
            SessionMode::EngineRunning => match kind {
                // Its own, valid engine: nothing to do; say so.
                InstallKind::Engine => self.send_manifest(now_ms, link),
                InstallKind::Core => self.hand_over_to_core_only(target, record),
            },
            SessionMode::CoreOnly => self.offer_in_core_only(target, now_ms, link, record),
        }
    }

    /// Steps 1–7 of the module docs: everything but fit and ownership.
    fn check_offer<T: UpdateTarget>(
        &self,
        _target: &T,
        link: LinkId,
        offer: &Offer,
    ) -> Result<InstallKind, Refusal> {
        let unknown = unknown_must_understand(offer.flags, OFFER_FLAGS_KNOWN_V1);
        if unknown != 0 {
            return Err(Refusal::Incompatible {
                what: Mismatch::Flags,
                have: 0,
                need: u16::from(unknown),
            });
        }
        if !self.facts.trusted_boot {
            return Err(Refusal::Untrusted);
        }
        let facts = &self.facts;
        for (what, have, need, ok) in [
            (
                Mismatch::Chip,
                facts.chip,
                offer.chip,
                offer.chip == facts.chip,
            ),
            (
                Mismatch::Layout,
                facts.layout,
                offer.layout,
                offer.layout == facts.layout,
            ),
            (
                Mismatch::Loader,
                facts.loader,
                offer.min_loader,
                facts.loader >= offer.min_loader,
            ),
        ] {
            if !ok {
                return Err(Refusal::Incompatible { what, have, need });
            }
        }
        let kind = install_kind(
            &offer.core_sha256,
            &offer.engine_sha256,
            &facts.core_sha256,
            &facts.digest_slot,
        )
        .map_err(|_| Refusal::HashMismatch)?;
        let op = match kind {
            InstallKind::Engine => Operation::EngineInstall,
            InstallKind::Core => Operation::CoreInstall,
        };
        if !self.link_may(link, op) {
            return Err(Refusal::Access);
        }
        if self.on_trial {
            return Err(Refusal::Untrusted);
        }
        if kind == InstallKind::Core && Some(offer.build_hash()) == facts.refused_build {
            return Err(Refusal::FailedBuild {
                build_hash: offer.build_hash(),
            });
        }
        Ok(kind)
    }

    /// The fit check (E8) and the record the transfer would write.
    fn fit<T: UpdateTarget>(
        &self,
        target: &T,
        offer: &Offer,
        kind: InstallKind,
    ) -> Result<TransferRecord, Refusal> {
        match kind {
            InstallKind::Core => {
                let room_beside = self.facts.region_len.saturating_sub(self.facts.core_len);
                let Some(dest) = target.core_dest(offer.core_len) else {
                    return Err(Refusal::DoesNotFit {
                        need: offer.core_len.saturating_add(offer.engine_len),
                        room: room_beside,
                    });
                };
                let room = target.engine_room_for(dest, offer.core_len);
                let room = room.end.saturating_sub(room.start);
                if offer.engine_len > room || offer.core_len == 0 {
                    return Err(Refusal::DoesNotFit {
                        need: offer.core_len.saturating_add(offer.engine_len),
                        room: offer.core_len.saturating_add(room),
                    });
                }
                Ok(TransferRecord {
                    kind: PieceKind::Core,
                    stage: RecordStage::Writing,
                    build: offer.build_hash(),
                    dest,
                    len: offer.core_len,
                    sha256: offer.core_sha256,
                })
            }
            InstallKind::Engine => {
                let room = &self.own_engine_room;
                let len = room.end.saturating_sub(room.start);
                if offer.engine_len > len || offer.engine_len == 0 {
                    return Err(Refusal::DoesNotFit {
                        need: offer.engine_len,
                        room: len,
                    });
                }
                Ok(TransferRecord {
                    kind: PieceKind::Engine,
                    stage: RecordStage::Writing,
                    build: self.facts.build_hash(),
                    dest: room.start,
                    len: offer.engine_len,
                    // The engine is accepted by the core's own digest, never
                    // by what an offer claims (equal here, by the install
                    // kind's rule).
                    sha256: self.facts.digest_slot,
                })
            }
        }
    }

    /// Core-only: start, resume, take over or refuse, by the owner rule.
    fn offer_in_core_only<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        now_ms: u64,
        link: LinkId,
        record: TransferRecord,
    ) {
        if let Some(t) = &self.transfer {
            let mine = t.owner == Some(link);
            if !mine && owner_live(&self.links, t.owner, now_ms, self.config.owner_quiet_ms) {
                let refusal = Refusal::Busy {
                    done: t.done_bytes(),
                    total: t.record.len,
                };
                return self.refuse(link, refusal);
            }
            if t.matches(&record) {
                return self.resume(target, link);
            }
            // A different offer replaces it (a heal cancels a pending core
            // transfer, E2); the record is overwritten when it starts.
            self.transfer = None;
        }
        self.start(target, link, record);
    }

    // ---- Shared helpers --------------------------------------------------------

    pub(super) fn send(&mut self, link: LinkId, bytes: Vec<u8>) {
        self.outbox.push(Outgoing { link, bytes });
    }

    pub(super) fn refuse(&mut self, link: LinkId, refusal: Refusal) {
        self.send(link, refusal.encode());
    }

    pub(super) fn send_manifest(&mut self, now_ms: u64, link: LinkId) {
        let json = self.manifest_for(now_ms, link).to_json();
        let mut bytes = Vec::with_capacity(1 + json.len());
        bytes.push(b'M');
        bytes.extend_from_slice(&json);
        self.send(link, bytes);
    }

    pub(super) fn link_may(&self, link: LinkId, op: Operation) -> bool {
        match self.links.iter().find(|l| l.id == link) {
            Some(l) => may(op, l.trust, l.granted, &self.access),
            None => may(op, LinkTrust::Untrusted, None, &self.access),
        }
    }

    pub(super) fn push_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    /// The chip is about to reset: ask for it and stop.
    pub(super) fn reset(&mut self) {
        self.effects.push(Effect::Reset);
        self.halted = true;
    }
}
