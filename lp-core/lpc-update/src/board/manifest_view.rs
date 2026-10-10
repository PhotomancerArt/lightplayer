//! The board manifest as the session reports it (DM8): every identity field
//! of `one-way-doors.md` §2 from [`BoardFacts`](super::BoardFacts), plus the
//! session's own state.
//!
//! | `state` | When |
//! |---|---|
//! | `updating` | a transfer is pending or running; `transfer` is filled |
//! | `on-trial` | an unconfirmed trial core |
//! | `engine-crashing` | the facts say so (E10) |
//! | `needs-engine` | core-only with no valid engine |
//! | `running` | the running engine |
//!
//! The first row that holds wins. `transfer.busy` is true when another link
//! owns the transfer and is live (E6): a host that sees it will get `N`/`B`.
//! `engineLen` is the facts' while the engine header is valid, and `null`
//! once the session has erased it (or when the core never knew it). The
//! core's SHA-256 arrives in the facts: the firmware computes and caches it
//! (DM24); the session never hashes the running core.

use crate::board_manifest::{BoardManifest, BoardState, TransferView};
use crate::board_manifest_view::BoardManifestView;
use crate::code_table::PROTO_V1;

use super::board_link::LinkId;
use super::board_session::BoardSession;
use super::transfer_owner::owner_live;
use super::update_target::{EngineStatus, SessionMode};

impl BoardSession {
    /// The manifest as `link` sees it at `now_ms`.
    #[must_use]
    pub fn manifest_for(&self, now_ms: u64, link: LinkId) -> BoardManifest {
        self.manifest_seen_by(now_ms, Some(link))
    }

    /// The manifest as a link that owns nothing sees it at `now_ms` (the
    /// hello's copy, Part B).
    #[must_use]
    pub fn manifest(&self, now_ms: u64) -> BoardManifest {
        self.manifest_seen_by(now_ms, None)
    }

    /// The same manifest as [`manifest`](Self::manifest), borrowing its text
    /// from the facts: nothing on the heap, so a board can keep it resident.
    #[must_use]
    pub fn manifest_view(&self, now_ms: u64) -> BoardManifestView<'_> {
        self.view_seen_by(now_ms, None)
    }

    fn manifest_seen_by(&self, now_ms: u64, link: Option<LinkId>) -> BoardManifest {
        self.view_seen_by(now_ms, link).to_manifest()
    }

    fn view_seen_by(&self, now_ms: u64, link: Option<LinkId>) -> BoardManifestView<'_> {
        let f = &self.facts;
        let transfer = self.transfer.as_ref().map(|t| TransferView {
            kind: t.kind(),
            done: t.done_bytes(),
            total: t.record.len,
            busy: t.owner != link
                && owner_live(&self.links, t.owner, now_ms, self.config.owner_quiet_ms),
            build_hash: t.record.build,
        });
        let state = if transfer.is_some() {
            BoardState::Updating
        } else if self.on_trial {
            BoardState::OnTrial
        } else if f.mode == SessionMode::EngineRunning {
            BoardState::Running
        } else if f.engine == EngineStatus::Crashing && self.engine_valid {
            BoardState::EngineCrashing
        } else {
            BoardState::NeedsEngine
        };
        BoardManifestView {
            proto: PROTO_V1,
            target: &f.target,
            chip: &f.chip_word,
            version: &f.version,
            build_id: f.build_id,
            wire_proto: f.wire_proto,
            core_sha256: f.core_sha256,
            core_len: f.core_len,
            engine_sha256: f.digest_slot,
            engine_len: f.engine_len.filter(|_| self.engine_valid),
            layout: f.layout,
            loader: f.loader,
            region_len: f.region_len,
            state,
            refused_build: f.refused_build,
            transfer,
        }
    }
}
