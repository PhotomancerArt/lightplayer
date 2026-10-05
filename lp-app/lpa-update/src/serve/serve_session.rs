//! Serving one build to one board on one link: the session that answers
//! `R`.
//!
//! - **`offer()`** is the build's `O` (flags 0; the chip's `u16` from
//!   `lpc_update`'s code table).
//! - **On `R kind off len flags`:**
//!   - a must-understand bit (high 4) this host does not know → the request
//!     is not served, and [`ServeEvent::UnservableRequest`] reports it;
//!   - the requested chunk is answered, then up to `ahead − 1` more are kept
//!     in flight past it (send-ahead). A request for the chunk right after
//!     the previous one, already sent ahead, only tops the window up; any
//!     other request (the first, one behind the stream, a repeat) restarts
//!     the stream there;
//!   - the **engine header** (kind `E`, off 0) always goes alone;
//!   - flag bit 0 set → `Z` when the build has encoding 1 for that piece and
//!     the chunk's index length is non-zero, `D` otherwise; flag clear →
//!     always `D` (the board's raw fallback).
//! - **`N`** becomes [`ServeEvent::Refused`] with a [`HostRefusal`]; `M`
//!   becomes [`ServeEvent::Manifest`]. Board message types this host does not
//!   know are ignored: hosts are the newer side.
//! - **`proto` is information:** the session notes the board's (`M`) and
//!   sends only v1 messages, which every board speaks.

use alloc::vec::Vec;

use lpc_update::code_table::CHUNK;
use lpc_update::flag_rule::{REQUEST_FLAGS_KNOWN_V1, unknown_must_understand};
use lpc_update::{BoardManifest, BoardMessage, ChunkEncoding, PieceKind, Request, encode_chunk};

use crate::host_build::{HostBuild, HostPiece};
use crate::host_refusal::HostRefusal;

/// How far ahead to stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServeConfig {
    /// Chunks in flight per request (1 = one per request).
    pub ahead: u8,
}

impl ServeConfig {
    /// USB: one chunk per request.
    pub const USB: Self = Self { ahead: 1 };
    /// BLE: four in flight (the spike's S5c measurement).
    pub const BLE: Self = Self { ahead: 4 };
}

/// What happened, for the caller.
#[derive(Clone, Debug, PartialEq)]
pub enum ServeEvent {
    /// The board's manifest (`M`).
    Manifest(BoardManifest),
    /// The board refused (`N`).
    Refused(HostRefusal),
    /// A request carried a must-understand flag this host does not know; it
    /// was not served.
    UnservableRequest { flags: u8 },
    /// A request for bytes the build does not have; not served.
    RequestOutOfRange(Request),
}

/// The bytes served, for `lp-cli` and the ship report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServeCounters {
    pub requests: u32,
    pub chunks_raw: u32,
    pub chunks_encoded: u32,
    /// Payload bytes sent raw (`D`).
    pub bytes_raw: u64,
    /// Payload bytes sent in encoding 1 (`Z`).
    pub bytes_encoded: u64,
    /// Chunks sent again after the first time, and their payload bytes.
    pub chunks_duplicate: u32,
    pub bytes_duplicate: u64,
}

/// What [`ServeSession::on_board`] gives back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServeOutput {
    /// Messages for the board, in order.
    pub send: Vec<Vec<u8>>,
    pub events: Vec<ServeEvent>,
}

/// The serving half of an update. See the module docs.
#[derive(Clone, Debug)]
pub struct ServeSession {
    config: ServeConfig,
    /// The last request, as `(kind, chunk)`.
    last_request: Option<(PieceKind, u32)>,
    /// The next chunk the stream would send, as `(kind, chunk)`.
    frontier: Option<(PieceKind, u32)>,
    sent_core: Vec<bool>,
    sent_engine: Vec<bool>,
    board_proto: Option<u8>,
    counters: ServeCounters,
}

impl ServeSession {
    #[must_use]
    pub fn new(config: ServeConfig) -> Self {
        Self {
            config,
            last_request: None,
            frontier: None,
            sent_core: Vec::new(),
            sent_engine: Vec::new(),
            board_proto: None,
            counters: ServeCounters::default(),
        }
    }

    /// The build's `O`.
    #[must_use]
    pub fn offer(build: &HostBuild) -> Vec<u8> {
        build.offer().encode()
    }

    #[must_use]
    pub fn counters(&self) -> ServeCounters {
        self.counters
    }

    /// The protocol version the board reported, once it has.
    #[must_use]
    pub fn board_proto(&self) -> Option<u8> {
        self.board_proto
    }

    /// One board message.
    pub fn on_board(&mut self, build: &HostBuild, bytes: &[u8]) -> ServeOutput {
        let mut out = ServeOutput::default();
        match BoardMessage::decode(bytes) {
            Ok(BoardMessage::Request(r)) => self.on_request(build, r, &mut out),
            Ok(BoardMessage::Refusal(n)) => out.events.push(ServeEvent::Refused(n.into())),
            Ok(BoardMessage::Manifest(json)) => {
                if let Ok(m) = BoardManifest::from_json(json) {
                    self.board_proto = Some(m.proto);
                    out.events.push(ServeEvent::Manifest(m));
                }
            }
            // Read-back data and login steps are other sessions'; unknown
            // board messages are ignored (hosts are the newer side).
            _ => {}
        }
        out
    }

    fn on_request(&mut self, build: &HostBuild, r: Request, out: &mut ServeOutput) {
        self.counters.requests += 1;
        let unknown = unknown_must_understand(r.flags, REQUEST_FLAGS_KNOWN_V1);
        if unknown != 0 {
            out.events
                .push(ServeEvent::UnservableRequest { flags: r.flags });
            return;
        }
        let piece = build.piece(r.kind);
        let idx = r.off / CHUNK;
        let in_range = r.off % CHUNK == 0
            && piece
                .raw_chunk(idx)
                .is_some_and(|c| c.len() as u32 == r.len);
        if !in_range {
            out.events.push(ServeEvent::RequestOutOfRange(r));
            return;
        }
        let takes_z = r.takes_encoding_1();
        let header = r.kind == PieceKind::Engine && idx == 0;
        let continues = idx > 0
            && self.last_request == Some((r.kind, idx - 1))
            && self.frontier.is_some_and(|(k, f)| k == r.kind && f > idx);
        self.last_request = Some((r.kind, idx));
        let mut next = if continues {
            self.frontier.map_or(idx, |(_, f)| f)
        } else {
            self.send(piece, r.kind, idx, takes_z, out);
            idx + 1
        };
        if header {
            // The header goes alone, and ends the stream.
            self.frontier = None;
            return;
        }
        let window_end = idx + u32::from(self.config.ahead.max(1));
        while next < window_end && next < piece.chunk_count() {
            self.send(piece, r.kind, next, takes_z, out);
            next += 1;
        }
        self.frontier = Some((r.kind, next));
    }

    fn send(
        &mut self,
        piece: &HostPiece,
        kind: PieceKind,
        idx: u32,
        takes_z: bool,
        out: &mut ServeOutput,
    ) {
        let off = idx * CHUNK;
        let (encoding, payload) = match piece.encoded_chunk(idx).filter(|_| takes_z) {
            Some(z) => (ChunkEncoding::Encoding1, z),
            None => (ChunkEncoding::Raw, piece.raw_chunk(idx).unwrap_or_default()),
        };
        let sent = match kind {
            PieceKind::Core => &mut self.sent_core,
            PieceKind::Engine => &mut self.sent_engine,
        };
        if sent.len() <= idx as usize {
            sent.resize(idx as usize + 1, false);
        }
        let n = payload.len() as u64;
        if sent[idx as usize] {
            self.counters.chunks_duplicate += 1;
            self.counters.bytes_duplicate += n;
        }
        sent[idx as usize] = true;
        match encoding {
            ChunkEncoding::Raw => {
                self.counters.chunks_raw += 1;
                self.counters.bytes_raw += n;
            }
            ChunkEncoding::Encoding1 => {
                self.counters.chunks_encoded += 1;
                self.counters.bytes_encoded += n;
            }
        }
        out.send.push(encode_chunk(encoding, kind, off, payload));
    }
}
