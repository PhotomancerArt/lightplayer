//! The host × board world: an `UpdateDriver` (this crate) talking to a
//! `BoardSession` (lpc-update) on the NOR model, through a pipe that can
//! drop the link, deliver out of order, duplicate or corrupt a chunk, and a
//! board whose power can be cut after any flash operation or after the
//! host served its Nth request.
//!
//! The pipe models lp-link's channel 3, which is reliable and ordered
//! (DM30): within one link nothing is lost or reordered unless a fault says
//! so, and a dropped link loses what was in flight, both ends seeing it go
//! down. A stall (both queues empty, the driver not done) is what a host
//! sees as an idle link: the world drops it, and the next link starts over
//! from `Q` — that is the host's only recovery, so it is the one tested.

use std::collections::{HashMap, VecDeque};

use lpa_update::decide::{SourceEffect, SourceResult, StoreAnswer};
use lpa_update::{
    Credential, DriverConfig, DriverEffect, EncodedPiece, Finish, HostBuild, HostIdentity,
    UpdateDriver,
};
use lpc_access::Tier;
use lpc_update::board::{AccessFacts, LinkId, LinkTrust, SessionConfig, SessionMode};
use lpc_update::code_table::CHUNK;
use lpc_update::testing::{BoardRig, BootFault, FakeBoard, MODEL_PROGRESS, ModelBuild};
use lpc_update::transfer_record::{RecordRead, TransferRecord};
use lpc_update::{BoardMessage, ChunkEncoding, HostMessage, PieceKind, encode_chunk};

/// Faults to inject.
#[derive(Clone, Copy, Debug, Default)]
pub struct Faults {
    /// Drop the link after every K messages delivered (either way).
    pub drop_every: Option<u32>,
    /// Deliver each batch of chunks the host sends in reverse order.
    pub reorder: bool,
    /// Deliver this chunk twice, once.
    pub duplicate: Option<(PieceKind, u32)>,
    /// Flip a byte of this `D`, once.
    pub corrupt_d: Option<(PieceKind, u32)>,
    /// Truncate this `Z`, once.
    pub corrupt_z: Option<(PieceKind, u32)>,
    /// Cut the board's power right after the host serves its Nth request.
    pub cut_after_request: Option<u32>,
    /// Cut the board's power after this many flash operations.
    pub cut_after_op: Option<u64>,
    /// Half-do the operation a cut lands in.
    pub tear: bool,
}

/// How a run ended.
#[derive(Debug)]
pub struct Outcome {
    pub finish: Option<Finish>,
    pub cuts: u32,
    pub link_drops: u32,
    pub stalls: u32,
    /// Chunks the host served (raw and encoded, duplicates included).
    pub chunks_served: u32,
    /// At the first cut: the progress record's piece, length and marks.
    pub at_cut: Option<(PieceKind, u32, u32)>,
    /// Times the driver asked for credentials (a core-side login).
    pub logins: u32,
}

/// The world.
pub struct World {
    pub rig: BoardRig,
    pub driver: UpdateDriver,
    link: Option<LinkId>,
    next_link: u32,
    pub trust: LinkTrust,
    /// The tier the running engine's server holds for this host (channel 1).
    pub engine_tier: Option<Tier>,
    now: u64,
    to_board: VecDeque<Vec<u8>>,
    to_host: VecDeque<Vec<u8>>,
    pub cache: HashMap<[u8; 32], Vec<u8>>,
    pub store: HashMap<[u8; 32], Vec<u8>>,
    pub credentials: Vec<Credential>,
    pub faults: Faults,
    delivered: u32,
    /// Requests the host has answered.
    served_requests: u32,
    out: Outcome,
}

/// The `HostBuild` of a model build, with encoding 1 when asked (and the
/// packer is built in).
pub fn host_build(b: &ModelBuild, encoded: bool) -> HostBuild {
    let identity = HostIdentity {
        target: "esp32c6-4mb".into(),
        chip: "esp32c6".into(),
        version: b.version.clone(),
        build_id: b.build_id.clone(),
        wire_proto: 36,
        layout: 1,
        min_loader: 1,
    };
    let (core_z, engine_z) = if encoded { pack(b) } else { (None, None) };
    HostBuild::from_parts(identity, b.core.clone(), b.engine.clone(), core_z, engine_z).unwrap()
}

#[cfg(feature = "pack")]
fn pack(b: &ModelBuild) -> (Option<EncodedPiece>, Option<EncodedPiece>) {
    use lpa_update::pack::pack_piece;
    (
        Some(pack_piece(PieceKind::Core, &b.core)),
        Some(pack_piece(PieceKind::Engine, &b.engine)),
    )
}

#[cfg(not(feature = "pack"))]
fn pack(_: &ModelBuild) -> (Option<EncodedPiece>, Option<EncodedPiece>) {
    panic!("encoding 1 cases need --features pack")
}

impl World {
    pub fn new(
        board: FakeBoard,
        access: AccessFacts,
        build: HostBuild,
        config: DriverConfig,
        trust: LinkTrust,
    ) -> Self {
        let session = SessionConfig {
            entropy: Some(|b: &mut [u8]| b.fill(0x3c)),
            ..SessionConfig::default()
        };
        let rig = BoardRig::new(board, access, session).expect("the board boots");
        let mut driver = UpdateDriver::new(build, config);
        driver.go();
        Self {
            rig,
            driver,
            link: None,
            next_link: 10,
            trust,
            engine_tier: None,
            now: 0,
            to_board: VecDeque::new(),
            to_host: VecDeque::new(),
            cache: HashMap::new(),
            store: HashMap::new(),
            credentials: Vec::new(),
            faults: Faults::default(),
            delivered: 0,
            served_requests: 0,
            out: Outcome {
                finish: None,
                cuts: 0,
                link_drops: 0,
                stalls: 0,
                chunks_served: 0,
                at_cut: None,
                logins: 0,
            },
        }
    }

    /// Run until the driver finishes (or `max_steps`).
    pub fn run(mut self, max_steps: u32) -> (Outcome, World) {
        if let Some(k) = self.faults.cut_after_op {
            self.rig.board.flash.tear(self.faults.tear);
            self.rig.board.flash.cut_after(k);
        }
        for _ in 0..max_steps {
            if self.out.finish.is_some() {
                break;
            }
            self.now += 1;
            self.step();
            assert!(self.out.stalls < 200, "stalled: {:?}", self.out);
        }
        let s = self.driver.served();
        self.out.chunks_served = s.chunks_raw + s.chunks_encoded;
        let out = std::mem::replace(
            &mut self.out,
            Outcome {
                finish: None,
                cuts: 0,
                link_drops: 0,
                stalls: 0,
                chunks_served: 0,
                at_cut: None,
                logins: 0,
            },
        );
        (out, self)
    }

    fn step(&mut self) {
        let Some(link) = self.link else {
            return self.bring_link_up();
        };
        if let Some(bytes) = self.to_board.pop_front() {
            if self.count_delivery() {
                return; // the link dropped with this message in flight
            }
            let outs = self.rig.deliver(self.now, link, self.engine_tier, &bytes);
            for o in outs {
                self.to_host.push_back(o.bytes);
            }
            self.after_board();
        } else if let Some(bytes) = self.to_host.pop_front() {
            if self.count_delivery() {
                return;
            }
            let is_request = matches!(BoardMessage::decode(&bytes), Ok(BoardMessage::Request(_)));
            let creds = self.credentials.clone();
            self.driver.on_board(self.now, &bytes, &creds);
            self.pump();
            self.served_requests += u32::from(is_request);
            if is_request && Some(self.served_requests) == self.faults.cut_after_request {
                // The host served request N: the power goes before the
                // board's next flash operation.
                self.faults.cut_after_request = None;
                self.rig.board.flash.cut_after(0);
            }
        } else {
            // Nothing in flight and not done: an idle link, dropped.
            self.out.stalls += 1;
            self.drop_link();
        }
    }

    /// Count one delivery; `true` when the link drops instead.
    fn count_delivery(&mut self) -> bool {
        self.delivered += 1;
        if let Some(k) = self.faults.drop_every
            && self.delivered % k == 0
        {
            self.out.link_drops += 1;
            self.drop_link();
            return true;
        }
        false
    }

    fn bring_link_up(&mut self) {
        let link = LinkId(self.next_link);
        self.next_link += 1;
        self.link = Some(link);
        for o in self.rig.link_up(self.now, link, self.trust) {
            self.to_host.push_back(o.bytes);
        }
        self.driver.link_up(self.now);
        self.pump();
    }

    fn drop_link(&mut self) {
        if let Some(link) = self.link.take() {
            self.rig.link_down(self.now, link);
        }
        self.driver.link_down(self.now);
        self.to_board.clear();
        self.to_host.clear();
        self.pump();
    }

    /// What the board did after a message: a reset or a power cut.
    fn after_board(&mut self) {
        if self.rig.board.flash.is_frozen() {
            return self.power_cut();
        }
        if self.rig.reset_pending {
            self.drop_link();
            match self.rig.reboot() {
                Ok(()) => {}
                // The cut landed in the boot itself (a trial's tally).
                Err(BootFault::PowerCut) => self.power_cut(),
                Err(e) => panic!("boots after a reset: {e:?}"),
            }
        }
    }

    /// The power is gone: check what the flash froze as, then boot again.
    fn power_cut(&mut self) {
        self.out.cuts += 1;
        if self.out.at_cut.is_none() {
            self.out.at_cut = marked_chunks(&self.rig.board);
        }
        self.rig
            .board
            .check_invariant()
            .unwrap_or_else(|e| panic!("after a cut: {e:?}"));
        self.drop_link();
        self.rig.power_cycle().expect("boots after a cut");
    }

    /// Perform the driver's effects until it asks for nothing more.
    fn pump(&mut self) {
        loop {
            let effects = self.driver.take_effects();
            if effects.is_empty() {
                return;
            }
            let mut chunks = Vec::new();
            for e in effects {
                match e {
                    DriverEffect::Send(bytes) => {
                        if is_chunk(&bytes) {
                            chunks.push(bytes);
                        } else {
                            self.flush_chunks(&mut chunks);
                            self.to_board.push_back(bytes);
                        }
                    }
                    DriverEffect::Source(SourceEffect::LookUpCache { sha }) => {
                        let hit = self.cache.get(&sha).cloned();
                        self.driver.source_result(SourceResult::Cache(hit));
                    }
                    DriverEffect::Source(SourceEffect::FetchFromStore { sha, .. }) => {
                        let answer = match self.store.get(&sha) {
                            Some(b) => StoreAnswer::Found(b.clone()),
                            None => StoreAnswer::NotFound,
                        };
                        self.driver.source_result(SourceResult::Store(answer));
                    }
                    DriverEffect::Source(SourceEffect::KeepInCache { sha, bytes }) => {
                        self.cache.insert(sha, bytes);
                    }
                    DriverEffect::Source(SourceEffect::ReadBack { .. }) => {
                        unreachable!("the driver reads back itself")
                    }
                    DriverEffect::NeedCredentials => {
                        self.out.logins += 1;
                        let creds = self.credentials.clone();
                        self.driver.login_with(&creds);
                    }
                    DriverEffect::Done(f) => self.out.finish = Some(f),
                    DriverEffect::Progress { .. } | DriverEffect::Decided(_) => {}
                }
            }
            self.flush_chunks(&mut chunks);
        }
    }

    /// One batch of chunks onto the pipe, with the chunk faults.
    fn flush_chunks(&mut self, chunks: &mut Vec<Vec<u8>>) {
        if self.faults.reorder && chunks.len() > 1 {
            chunks.reverse();
        }
        for mut bytes in chunks.drain(..) {
            let Ok(HostMessage::Chunk(c)) = HostMessage::decode(&bytes) else {
                continue;
            };
            let at = (c.kind, c.off / CHUNK);
            let (encoding, kind, off) = (c.encoding, c.kind, c.off);
            if encoding == ChunkEncoding::Raw && self.faults.corrupt_d == Some(at) {
                self.faults.corrupt_d = None;
                let mid = 6 + (bytes.len() - 6) / 2;
                bytes[mid] ^= 0x40;
            }
            if encoding == ChunkEncoding::Encoding1 && self.faults.corrupt_z == Some(at) {
                self.faults.corrupt_z = None;
                let payload = bytes[6..].to_vec();
                bytes = encode_chunk(encoding, kind, off, &payload[..payload.len() / 2]);
            }
            if self.faults.duplicate == Some(at) {
                self.faults.duplicate = None;
                self.to_board.push_back(bytes.clone());
            }
            self.to_board.push_back(bytes);
        }
    }

    /// The board's state, for assertions.
    pub fn running(&self) -> (Option<&ModelBuild>, bool, Option<SessionMode>) {
        (
            self.rig.board.running_build(),
            self.rig.board.engine_valid(),
            self.rig.mode(),
        )
    }
}

fn is_chunk(bytes: &[u8]) -> bool {
    matches!(bytes.first(), Some(b'D' | b'Z'))
}

/// The progress record's piece, length and marks, if any.
pub fn marked_chunks(board: &FakeBoard) -> Option<(PieceKind, u32, u32)> {
    let at = MODEL_PROGRESS as usize;
    match TransferRecord::read(&board.flash.bytes()[at..at + CHUNK as usize]) {
        RecordRead::V1(r, marks) => Some((r.kind, r.len, marks.count())),
        _ => None,
    }
}

/// Access as a fresh board has it: open at Author, no secrets.
pub fn open_access() -> AccessFacts {
    AccessFacts::from_store(None)
}
