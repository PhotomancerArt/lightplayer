//! A small host for driving a [`BoardRig`] in tests: it offers one build and
//! answers the board's requests, raw or compressed.
//!
//! This is deliberately not `lpa-update`'s host (that crate depends on this
//! one): it is the least host that exercises the board's rules.

#![allow(dead_code, reason = "each test file uses part of the support")]

pub mod deflate_copy;

use std::collections::VecDeque;

use lpc_update::board::{AccessFacts, LinkId, LinkTrust, SessionConfig, SessionMode};
use lpc_update::code_table::{CHIP_ESP32C6, CHUNK, LAYOUT_1, LOADER_1, PROTO_V1};
use lpc_update::testing::{BoardRig, FakeBoard, ModelBuild};
use lpc_update::{
    BoardManifest, BoardMessage, ChunkEncoding, Offer, PieceKind, Refusal, Request, encode_chunk,
};

/// How the host answers a request that takes encoding 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZMode {
    /// Always `D`.
    Raw,
    /// `Z` without a dictionary (miniz), when smaller.
    NoDictionary,
    /// `Z` as one back-reference into the dictionary when the chunk repeats
    /// the one 4 KiB before it, otherwise as `NoDictionary`.
    CopyFromDictionary,
}

/// The test host.
pub struct Host {
    pub build: ModelBuild,
    pub z: ZMode,
    /// Corrupt this chunk once (flip one byte of a `D`, or garble a `Z`).
    pub corrupt_once: Option<(PieceKind, u32)>,
    /// Requests answered, by kind.
    pub served: Vec<Request>,
    /// Everything the board said, decoded.
    pub refusals: Vec<Refusal>,
    pub manifests: Vec<BoardManifest>,
    /// Extra chunks sent past each answer (send-ahead), out of order.
    pub ahead: u32,
}

impl Host {
    pub fn new(build: ModelBuild) -> Self {
        Self {
            build,
            z: ZMode::Raw,
            corrupt_once: None,
            served: Vec::new(),
            refusals: Vec::new(),
            manifests: Vec::new(),
            ahead: 0,
        }
    }

    pub fn offer(&self) -> Vec<u8> {
        offer_of(&self.build).encode()
    }

    fn piece(&self, kind: PieceKind) -> &[u8] {
        match kind {
            PieceKind::Core => &self.build.core,
            PieceKind::Engine => &self.build.engine,
        }
    }

    /// The host's answers to one board message.
    pub fn on_board(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        match BoardMessage::decode(bytes) {
            Ok(BoardMessage::Request(r)) => {
                self.served.push(r);
                let mut out = vec![self.chunk(r.kind, r.off, r.takes_encoding_1())];
                // Send-ahead: later chunks, newest first, so they arrive out
                // of order and before their turn.
                let piece_len = self.piece(r.kind).len() as u32;
                for k in (1..=self.ahead).rev() {
                    let off = r.off + k * CHUNK;
                    if off < piece_len && !(r.kind == PieceKind::Engine && r.off == 0) {
                        out.push(self.chunk(r.kind, off, r.takes_encoding_1()));
                    }
                }
                out
            }
            Ok(BoardMessage::Refusal(n)) => {
                self.refusals.push(n);
                Vec::new()
            }
            Ok(BoardMessage::Manifest(json)) => {
                self.manifests
                    .push(BoardManifest::from_json(json).expect("a manifest"));
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn chunk(&mut self, kind: PieceKind, off: u32, takes_z: bool) -> Vec<u8> {
        let piece = self.piece(kind);
        let end = (off + CHUNK).min(piece.len() as u32) as usize;
        let mut raw = piece[off as usize..end].to_vec();
        let corrupt = self.corrupt_once == Some((kind, off));
        let z = if takes_z {
            self.z_stream(kind, off, &raw)
        } else {
            None
        };
        if let Some(mut z) = z {
            if corrupt {
                self.corrupt_once = None;
                z.truncate(z.len() / 2);
            }
            return encode_chunk(ChunkEncoding::Encoding1, kind, off, &z);
        }
        if corrupt {
            self.corrupt_once = None;
            let mid = raw.len() / 2;
            raw[mid] ^= 0x40;
        }
        encode_chunk(ChunkEncoding::Raw, kind, off, &raw)
    }

    fn z_stream(&self, kind: PieceKind, off: u32, raw: &[u8]) -> Option<Vec<u8>> {
        let piece = self.piece(kind);
        let repeats = off >= CHUNK
            && lpc_update::dictionary_rule::dictionary(kind, off)
                .is_some_and(|d| d.start + CHUNK <= off)
            && piece[(off - CHUNK) as usize..(off - CHUNK) as usize + raw.len()] == *raw;
        let z = match self.z {
            ZMode::Raw => return None,
            ZMode::CopyFromDictionary if repeats => {
                deflate_copy::copy_stream(raw.len(), CHUNK as u16)
            }
            _ => miniz_oxide::deflate::compress_to_vec(raw, 9),
        };
        (z.len() < raw.len()).then_some(z)
    }
}

/// The offer of `build`.
pub fn offer_of(build: &ModelBuild) -> Offer {
    Offer {
        proto: PROTO_V1,
        flags: 0,
        chip: CHIP_ESP32C6,
        layout: LAYOUT_1,
        min_loader: LOADER_1,
        core_len: build.core.len() as u32,
        engine_len: build.engine.len() as u32,
        core_sha256: build.core_sha256(),
        engine_sha256: build.engine_sha256(),
        build_id: build.build_id_field(),
    }
}

/// The two builds most tests use: X on the board, Y on offer.
pub fn x_and_y() -> (ModelBuild, ModelBuild) {
    (
        ModelBuild::synthetic("2026.10.05-1", 1, 5 * 4096 + 300, 8 * 4096 + 77),
        ModelBuild::synthetic("2026.10.06-1", 2, 6 * 4096 + 11, 9 * 4096 + 1000),
    )
}

/// The region most tests use: room for two cores and an engine.
pub const REGION: u32 = 40 * 4096;

pub const USB: LinkId = LinkId(1);
pub const RADIO: LinkId = LinkId(2);
pub const RADIO_2: LinkId = LinkId(3);

/// A rig holding `catalog[on_board]`, trusted access defaults, `Z` on.
pub fn rig(catalog: Vec<ModelBuild>, on_board: usize) -> BoardRig {
    rig_with(
        catalog,
        on_board,
        AccessFacts::from_store(None),
        SessionConfig::default(),
    )
}

pub fn rig_with(
    catalog: Vec<ModelBuild>,
    on_board: usize,
    access: AccessFacts,
    config: SessionConfig,
) -> BoardRig {
    let board = FakeBoard::flashed_with(catalog, on_board, REGION);
    BoardRig::new(board, access, config).expect("a factory board boots")
}

/// Deliver `first` from `link`, then keep answering what the board says
/// until it goes quiet or asks for a reset. Returns the board's messages.
pub fn exchange(
    rig: &mut BoardRig,
    host: &mut Host,
    link: LinkId,
    first: Vec<u8>,
    now: &mut u64,
) -> Vec<Vec<u8>> {
    let mut seen = Vec::new();
    let mut to_board = VecDeque::from([first]);
    while let Some(m) = to_board.pop_front() {
        if rig.reset_pending || rig.board.flash.is_frozen() {
            break;
        }
        *now += 1;
        for out in rig.deliver(*now, link, None, &m) {
            assert_eq!(out.link, link, "a reply went to another link");
            to_board.extend(host.on_board(&out.bytes));
            seen.push(out.bytes);
        }
    }
    seen
}

/// What [`drive`] saw.
#[derive(Debug, Default)]
pub struct Drive {
    pub boots: u32,
    pub cuts: u32,
}

/// Drive the board to the host's build over `link` (trusted): bring the link
/// up, offer, answer, and follow every reset with a reboot and a new link.
/// A frozen flash (a power cut) is checked against the invariant, powered
/// back on and booted. Stops when the host's build runs its engine.
pub fn drive(
    rig: &mut BoardRig,
    host: &mut Host,
    link: LinkId,
    trust: LinkTrust,
    now: &mut u64,
) -> Drive {
    let mut d = Drive::default();
    for _ in 0..40 {
        if rig.board.flash.is_frozen() {
            d.cuts += 1;
            rig.board
                .check_invariant()
                .unwrap_or_else(|e| panic!("after a cut: {e:?}"));
            rig.power_cycle().expect("boots after a cut");
            d.boots += 1;
        } else if rig.reset_pending {
            rig.reboot().expect("boots after a reset");
            d.boots += 1;
        }
        if rig.mode() == Some(SessionMode::EngineRunning)
            && rig.board.running_build() == Some(&host.build)
        {
            return d;
        }
        for out in rig.link_up(*now, link, trust) {
            host.on_board(&out.bytes);
        }
        let offer = host.offer();
        exchange(rig, host, link, offer, now);
        rig.link_down(*now, link);
        *now += 1_000;
    }
    panic!("did not converge: {:?} {:?}", host.refusals, rig.mode());
}
