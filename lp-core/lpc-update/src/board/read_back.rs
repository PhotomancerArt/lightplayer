//! Read-back (DM17): a host backs the board's engine up before putting
//! another version on.
//!
//! `G E off len` is answered with one board→host `D E off bytes`: the engine
//! extent's bytes `[off, off + min(len, CHUNK))`, clamped to the engine's
//! length. Several `G`s may queue (the host pulls ahead); each is answered
//! in order, one chunk per `G`.
//!
//! - Served only while the engine header is valid — the running engine, and
//!   an engine-crashing core-only board (E10), so a crashing engine can be
//!   backed up before another version goes on. Otherwise `N`/`T`.
//! - Access: play or above, or a trusted link ([`super::access_rule`]);
//!   otherwise `N`/`A`.
//! - v1 reads back the engine only; a `G` for the core is `N`/`T`.
//! - A `G` past the engine's end is ignored.

use crate::chunk::{ChunkEncoding, encode_chunk};
use crate::code_table::CHUNK;
use crate::piece_kind::PieceKind;
use crate::read_back_request::ReadBackRequest;
use crate::refusal::Refusal;

use super::access_rule::Operation;
use super::board_link::LinkId;
use super::board_session::BoardSession;
use super::session_output::Effect;
use super::update_target::UpdateTarget;

impl BoardSession {
    pub(super) fn on_read_back<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        link: LinkId,
        g: &ReadBackRequest,
    ) {
        let engine_len = self.facts.engine_len.filter(|_| self.engine_valid);
        let Some(engine_len) = engine_len.filter(|_| g.kind == PieceKind::Engine) else {
            return self.refuse(link, Refusal::Untrusted);
        };
        if !self.link_may(link, Operation::ReadBack) {
            return self.refuse(link, Refusal::Access);
        }
        if g.off >= engine_len {
            return;
        }
        let n = g.len.min(CHUNK).min(engine_len - g.off) as usize;
        let at = self.own_engine_room.start + g.off;
        if target.read(at, &mut self.sector_buf[..n]).is_err() {
            return self.push_effect(Effect::FlashFault);
        }
        let bytes = encode_chunk(
            ChunkEncoding::Raw,
            PieceKind::Engine,
            g.off,
            &self.sector_buf[..n],
        );
        self.send(link, bytes);
    }
}
