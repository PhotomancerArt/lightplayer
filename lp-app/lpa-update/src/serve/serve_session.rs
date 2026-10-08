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

/// The bytes a backup asks for per `G` over Bluetooth: its `D` (6 B of
/// header plus these) is at most 1024 B, the C6's `SMALL_REPLY_BYTES`
/// (`fw-esp32-common`'s `radio_link_config.rs`), so the board copies each
/// answer into the radio link's own send ring. A whole 4 KiB chunk instead
/// stays in the board's one shared frame buffer until the link has cut it
/// into frames, and every reply on every link waits for that first — the
/// heartbeat every 5 s, a USB host's answers. On a slow central the wait
/// passed the board's 5 s deadline and it closed the link mid-backup
/// (2026-10-07 desk run d2: `a reply still not out of the frame buffer
/// after 4633 ms … closing`; the render loop stalled 2–3.5 s at every
/// heartbeat before that).
pub const BLE_READ_BACK_PIECE: u32 = 1016;

/// How far ahead to stream, and how a backup reads back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServeConfig {
    /// Chunks in flight per request (1 = one per request). A backup keeps
    /// the same bytes outstanding, in pieces of `read_back_piece`.
    pub ahead: u8,
    /// Bytes per read-back `G` (at most one chunk).
    pub read_back_piece: u32,
}

impl ServeConfig {
    /// USB: four in flight. On the bench C6 (2026-10-06) the next chunks
    /// then arrive while the board decodes and writes this one: a whole
    /// update 44.8 → 37.4 s against one per request, with the board erasing
    /// blocks ahead. The backup pulls four chunks ahead too.
    pub const USB: Self = Self {
        ahead: 4,
        read_back_piece: CHUNK,
    };
    /// BLE: four in flight (the spike's S5c measurement); the backup reads
    /// back in [`BLE_READ_BACK_PIECE`]s, sixteen ahead (the same ~16 KiB).
    pub const BLE: Self = Self {
        ahead: 4,
        read_back_piece: BLE_READ_BACK_PIECE,
    };
    /// The LAN (a board's Wi-Fi link, `lan:`): eight in flight. On the
    /// desk (OTA Wi-Fi plan P6, 2026-10-07: FC6 fixture-c6, the test access
    /// point, a Mac's lp-cli, the board's LAN window of 8, two runs each)
    /// the core and engine pieces together took 40.9 s at `ahead` 2, 35.5 s
    /// at 4 and 34.0 s at 8. A LAN host drains a whole 4 KiB read-back chunk
    /// in a few round trips, so the backup reads whole chunks.
    pub const LAN: Self = Self {
        ahead: 8,
        read_back_piece: CHUNK,
    };

    /// Through lightplayer.app's relay (`relay:`): four in flight, and the
    /// backup reads back in [`BLE_READ_BACK_PIECE`]s. Every window crosses
    /// the internet twice and the board's relay leg takes 2 KiB at a time
    /// (its TCP receive buffer), so more in flight only queues on the host;
    /// a whole 4 KiB read-back chunk would hold the board's one shared
    /// frame buffer for round trips while the engine renders, where a
    /// small piece goes through the link's own send ring (Bluetooth's
    /// reason). Not measured through a real relay yet.
    pub const RELAY: Self = Self {
        ahead: 4,
        read_back_piece: BLE_READ_BACK_PIECE,
    };

    /// `ahead` chunks (and a backup reading whole chunks).
    #[must_use]
    pub const fn ahead(ahead: u8) -> Self {
        Self {
            ahead,
            read_back_piece: CHUNK,
        }
    }

    /// How many read-back `G`s a backup keeps outstanding: `ahead` chunks'
    /// worth of pieces.
    #[must_use]
    pub fn read_back_ahead(&self) -> u8 {
        let piece = self.read_back_piece.clamp(1, CHUNK);
        let pieces = (u32::from(self.ahead.max(1)) * CHUNK).div_ceil(piece);
        u8::try_from(pieces).unwrap_or(u8::MAX)
    }
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
