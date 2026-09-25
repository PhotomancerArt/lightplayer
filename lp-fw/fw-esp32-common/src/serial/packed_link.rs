//! One link's packed-reply state: the encoding its host opted into, and the
//! learned table a packed link codes against (plan
//! `lp2025/2026-09-25-0006-learned-wire-dictionary`).
//!
//! The table exists only while a host has the link packed: it is allocated
//! when the server is about to answer an opt-in `packed` (decision D4 — a
//! board running a show with no Studio attached pays no RAM for it), and
//! freed when the link goes back to JSON. It is allocated once per opt-in,
//! never per frame and never on the read-assembly heap peak that OOM'd the
//! classic (see `server_msg::FRAME_BUF`). If the heap cannot hold it, the
//! answer becomes `json`.
//!
//! The table must stay in step with the host's twin after every loss the
//! board knows about, so every learned frame is tentative until it is sent:
//! the transport takes a [`Tentative`] before serializing and hands it back
//! to [`PackedLink::rolled_back`] when the write fails. Every accepted
//! opt-in starts a new table epoch with an empty table — which is also how a
//! host whose table lost step asks for a fresh start.

use alloc::boxed::Box;
use lp_json_pack::{LearnMark, LearnStore, LearnedTable};
use lpc_wire::WireEncoding;
use lpc_wire::server::ServerMsgBody;

/// One link's packed-reply state. See the module docs.
pub struct PackedLink {
    encoding: WireEncoding,
    table: Option<Box<LearnedTable>>,
    /// The epoch the next accepted opt-in starts. Wraps; a host only needs a
    /// *different* epoch to see a reset.
    next_epoch: u8,
}

/// Where a frame's learning started, for [`PackedLink::rolled_back`].
#[derive(Debug, Clone, Copy)]
pub struct Tentative(Option<LearnMark>);

impl Default for PackedLink {
    fn default() -> Self {
        Self::new()
    }
}

impl PackedLink {
    /// A JSON link with no table.
    pub const fn new() -> Self {
        Self {
            encoding: WireEncoding::Json,
            table: None,
            next_epoch: 1,
        }
    }

    /// The encoding the link's replies go out in.
    pub fn encoding(&self) -> WireEncoding {
        self.encoding
    }

    /// The table to code the next reply against: `Some` on a packed link,
    /// except for the answer to an opt-in, which is always JSON (the host
    /// reads it before it knows the outcome).
    pub fn table_for(&mut self, msg: &ServerMsgBody) -> Option<&mut dyn LearnStore> {
        if matches!(msg, ServerMsgBody::SetEncoding { .. }) {
            return None;
        }
        match self.encoding {
            WireEncoding::Packed => self.table.as_deref_mut().map(|t| t as &mut dyn LearnStore),
            WireEncoding::Json => None,
        }
    }

    /// Where the table stands before a reply is serialized.
    pub fn tentative(&self) -> Tentative {
        Tentative(match self.encoding {
            WireEncoding::Packed => self.table.as_deref().map(LearnStore::mark),
            WireEncoding::Json => None,
        })
    }

    /// The reply serialized after `tentative` was not written (the io task
    /// abandoned it): the host never decoded it whole, so it learned nothing
    /// from it either. Forget what the board learned from it.
    pub fn rolled_back(&mut self, tentative: Tentative) {
        if let (Some(mark), Some(table)) = (tentative.0, self.table.as_deref_mut()) {
            table.truncate(mark);
        }
    }

    /// The server is about to write this opt-in answer: make sure a
    /// `packed` answer can be honoured. Allocates the table if the link has
    /// none; if the heap cannot hold one, the answer becomes `json`.
    pub fn prepare_answer(&mut self, answer: &mut ServerMsgBody) {
        let ServerMsgBody::SetEncoding { encoding } = answer else {
            return;
        };
        if *encoding != WireEncoding::Packed || self.table.is_some() {
            return;
        }
        match LearnedTable::try_boxed() {
            Some(table) => self.table = Some(table),
            None => {
                log::warn!(
                    "packed link: no heap for the {} B learned table; answering json",
                    core::mem::size_of::<LearnedTable>()
                );
                *encoding = WireEncoding::Json;
            }
        }
    }

    /// The opt-in answer naming `encoding` was written: every reply after it
    /// is in that encoding. `packed` starts a new epoch with an empty table;
    /// `json` frees the table.
    pub fn answered(&mut self, encoding: WireEncoding) {
        match (encoding, self.table.as_deref_mut()) {
            (WireEncoding::Packed, Some(table)) => {
                table.reset(self.next_epoch);
                log::info!(
                    "packed link: replies are now packed (table epoch {})",
                    self.next_epoch
                );
                self.next_epoch = self.next_epoch.wrapping_add(1);
                self.encoding = WireEncoding::Packed;
            }
            _ => self.back_to_json(),
        }
    }

    /// The opt-in answer was not written: a table allocated for it and not
    /// in use is freed.
    pub fn answer_dropped(&mut self) {
        if self.encoding == WireEncoding::Json {
            self.table = None;
        }
    }

    /// The link closed (or its host went away): JSON again, table freed.
    pub fn back_to_json(&mut self) {
        if self.encoding != WireEncoding::Json {
            log::info!("packed link: replies are JSON again");
        }
        self.encoding = WireEncoding::Json;
        self.table = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::{WireServerMessage, ser_learned_frame_to};

    #[test]
    fn an_opt_in_allocates_starts_an_epoch_and_json_frees() {
        let mut link = PackedLink::new();
        assert!(link.table_for(&heartbeat()).is_none(), "JSON has no table");
        let mut answer = set_encoding(WireEncoding::Packed);
        link.prepare_answer(&mut answer);
        assert!(matches!(
            answer,
            ServerMsgBody::SetEncoding {
                encoding: WireEncoding::Packed
            }
        ));
        assert!(link.table_for(&answer).is_none(), "the answer is JSON");
        link.answered(WireEncoding::Packed);
        assert_eq!(link.encoding(), WireEncoding::Packed);
        assert_eq!(link.table_for(&heartbeat()).unwrap().epoch(), 1);

        // A second opt-in (the host's reset request): a new, empty epoch.
        send(&mut link, 1);
        link.prepare_answer(&mut set_encoding(WireEncoding::Packed));
        link.answered(WireEncoding::Packed);
        let table = link.table_for(&heartbeat()).unwrap();
        assert_eq!(table.epoch(), 2);
        assert_eq!(table.mark(), lp_json_pack::LearnMark::default());

        link.back_to_json();
        assert_eq!(link.encoding(), WireEncoding::Json);
        assert!(link.table.is_none(), "freed with the link");
    }

    /// The io task abandoned the write: the table forgets what it learned
    /// from that frame, so the next frame's header is the one the host holds.
    #[test]
    fn an_abandoned_write_rolls_the_table_back() {
        let mut link = packed_link();
        send(&mut link, 1);
        let before = link.table_for(&heartbeat()).unwrap().mark();
        let tentative = link.tentative();
        send(&mut link, 2);
        assert_ne!(link.table_for(&heartbeat()).unwrap().mark(), before);
        link.rolled_back(tentative);
        assert_eq!(link.table_for(&heartbeat()).unwrap().mark(), before);
    }

    #[test]
    fn a_dropped_answer_frees_a_table_not_yet_in_use() {
        let mut link = PackedLink::new();
        link.prepare_answer(&mut set_encoding(WireEncoding::Packed));
        assert!(link.table.is_some());
        link.answer_dropped();
        assert!(link.table.is_none());

        // On a packed link the table stays: it is in use.
        let mut link = packed_link();
        link.prepare_answer(&mut set_encoding(WireEncoding::Packed));
        link.answer_dropped();
        assert!(link.table.is_some());
    }

    fn packed_link() -> PackedLink {
        let mut link = PackedLink::new();
        link.prepare_answer(&mut set_encoding(WireEncoding::Packed));
        link.answered(WireEncoding::Packed);
        link
    }

    /// Serialize a reply that teaches the table (a new log line each time).
    fn send(link: &mut PackedLink, n: u64) {
        let msg = WireServerMessage::new(
            n,
            ServerMsgBody::Log {
                level: lpc_wire::server::api::LogLevel::Info,
                message: alloc::format!("line {n}"),
            },
        );
        let mut buf = alloc::vec![0u8; 512];
        let table = link.table_for(&msg.msg).expect("packed");
        ser_learned_frame_to(&mut buf, table, &msg).unwrap();
    }

    fn heartbeat() -> ServerMsgBody {
        ServerMsgBody::StopAllProjects
    }

    fn set_encoding(encoding: WireEncoding) -> ServerMsgBody {
        ServerMsgBody::SetEncoding { encoding }
    }
}
