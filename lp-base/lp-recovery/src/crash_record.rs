//! Fixed-size crash record: what the panic path knows, written zero-alloc.

use core::sync::atomic::{Ordering, compiler_fence};

use crate::frame_kind::FrameKind;
use crate::frame_path::{FramePath, MAX_FRAME_DEPTH};
use crate::frame_record::truncation_boundary;

/// Bytes of crash message stored (truncated beyond this).
pub const CRASH_MSG_CAP: usize = 192;

/// PC frames kept from the backtrace capture.
pub const CRASH_PC_CAP: usize = 8;

/// Bytes of per-frame display name kept in the crash record's compact
/// path-name snapshot (shorter than the live stack's 24 to fit the region
/// budget; blame identity is unaffected — it lives in [`FramePath`] hashes).
pub const CRASH_FRAME_NAME_CAP: usize = 14;

const STATE_EMPTY: u32 = 0;
const STATE_TENTATIVE: u32 = 1;
const STATE_FINAL: u32 = 2;

/// What category of failure a crash record describes.
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CrashCause {
    Panic = 1,
    Oom = 2,
    /// Attributed after the fact: a watchdog reset with a live frame stack.
    Watchdog = 3,
    Unknown = 4,
}

impl CrashCause {
    pub fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(Self::Panic),
            2 => Some(Self::Oom),
            3 => Some(Self::Watchdog),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::Oom => "oom",
            Self::Watchdog => "watchdog",
            Self::Unknown => "unknown",
        }
    }
}

/// Heap statistics captured on OOM (zeros when not an OOM).
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct OomStats {
    pub requested: u32,
    pub align: u32,
    pub free: u32,
    pub used: u32,
}

/// A fixed-capacity crash message, always valid UTF-8 by construction.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CrashMsg {
    len: u16,
    _pad: [u8; 2],
    bytes: [u8; CRASH_MSG_CAP],
}

impl CrashMsg {
    pub const EMPTY: Self = Self {
        len: 0,
        _pad: [0; 2],
        bytes: [0; CRASH_MSG_CAP],
    };

    pub fn as_str(&self) -> &str {
        let len = (self.len as usize).min(CRASH_MSG_CAP);
        core::str::from_utf8(&self.bytes[..len]).unwrap_or("")
    }

    fn clear(&mut self) {
        self.len = 0;
    }
}

/// `core::fmt::Write` into a `CrashMsg`, truncating at capacity. Zero-alloc.
struct CrashMsgWriter<'a>(&'a mut CrashMsg);

impl core::fmt::Write for CrashMsgWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let len = self.0.len as usize;
        if len >= CRASH_MSG_CAP {
            return Ok(()); // silently truncate; never error out of panic-path formatting
        }
        let room = CRASH_MSG_CAP - len;
        let take = truncation_boundary(s, room);
        self.0.bytes[len..len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.0.len = (len + take) as u16;
        Ok(())
    }
}

/// Compact display name for one frame of the crashed path.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CompactFrameName {
    kind: u8,
    name_len: u8,
    name: [u8; CRASH_FRAME_NAME_CAP],
}

impl CompactFrameName {
    pub const EMPTY: Self = Self {
        kind: 0,
        name_len: 0,
        name: [0; CRASH_FRAME_NAME_CAP],
    };

    pub(crate) fn set(&mut self, kind_raw: u8, name: &str) {
        let end = truncation_boundary(name, CRASH_FRAME_NAME_CAP);
        self.kind = kind_raw;
        self.name_len = end as u8;
        self.name = [0; CRASH_FRAME_NAME_CAP];
        self.name[..end].copy_from_slice(&name.as_bytes()[..end]);
    }

    pub fn kind(&self) -> Option<FrameKind> {
        FrameKind::from_u8(self.kind)
    }

    pub fn name(&self) -> &str {
        let len = (self.name_len as usize).min(CRASH_FRAME_NAME_CAP);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }
}

/// The crash record as stored in the persistent region.
///
/// Lifecycle: `Empty → Tentative` (staged at panic-handler entry) →
/// either `Empty` again (layer-1 recovery caught the panic) or `Final`
/// (unwinding failed; we are about to reset). Watchdog-attributed records
/// are synthesized directly to `Final` on the next boot.
///
/// Torn-write discipline: all payload fields are written first, then the
/// `state` word flips last (with a compiler fence). A reset mid-stage
/// leaves `state == EMPTY` and the half-written payload is never read.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CrashRecord {
    state: u32,
    cause: u8,
    _pad: [u8; 3],
    msg: CrashMsg,
    pc_count: u32,
    pc_frames: [u32; CRASH_PC_CAP],
    heap: OomStats,
    path: FramePath,
    path_names: [CompactFrameName; MAX_FRAME_DEPTH],
    generation: u32,
}

impl CrashRecord {
    pub const EMPTY: Self = Self {
        state: STATE_EMPTY,
        cause: 0,
        _pad: [0; 3],
        msg: CrashMsg::EMPTY,
        pc_count: 0,
        pc_frames: [0; CRASH_PC_CAP],
        heap: OomStats {
            requested: 0,
            align: 0,
            free: 0,
            used: 0,
        },
        path: FramePath::EMPTY,
        path_names: [CompactFrameName::EMPTY; MAX_FRAME_DEPTH],
        generation: 0,
    };

    pub fn is_empty(&self) -> bool {
        self.state == STATE_EMPTY
    }

    pub fn is_tentative(&self) -> bool {
        self.state == STATE_TENTATIVE
    }

    pub fn is_final(&self) -> bool {
        self.state == STATE_FINAL
    }

    pub fn cause(&self) -> CrashCause {
        CrashCause::from_u8(self.cause).unwrap_or(CrashCause::Unknown)
    }

    pub fn msg(&self) -> &CrashMsg {
        &self.msg
    }

    pub fn pc_frames(&self) -> &[u32] {
        &self.pc_frames[..(self.pc_count as usize).min(CRASH_PC_CAP)]
    }

    pub fn heap(&self) -> OomStats {
        self.heap
    }

    pub fn path(&self) -> FramePath {
        self.path
    }

    pub fn path_names(&self) -> &[CompactFrameName] {
        &self.path_names[..self.path.len()]
    }

    /// Which boot generation the crash happened in.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Write payload and flip to `Tentative`. Zero-alloc; safe to call from
    /// a panic handler. Overwrites whatever was there (a newer crash always
    /// wins over an older record).
    pub(crate) fn stage(
        &mut self,
        cause: CrashCause,
        msg: &dyn core::fmt::Display,
        location: Option<(&str, u32)>,
        pcs: &[u32],
        heap: Option<OomStats>,
        path: FramePath,
        names: &[CompactFrameName],
        generation: u32,
    ) {
        use core::fmt::Write as _;

        self.state = STATE_EMPTY; // invalidate while rewriting payload
        compiler_fence(Ordering::SeqCst);

        self.cause = cause as u8;
        self.msg.clear();
        let mut w = CrashMsgWriter(&mut self.msg);
        // Errors are impossible (writer never fails), but never panic here.
        let _ = write!(w, "{msg}");
        if let Some((file, line)) = location {
            let _ = write!(w, " (at {file}:{line})");
        }
        let count = pcs.len().min(CRASH_PC_CAP);
        self.pc_frames = [0; CRASH_PC_CAP];
        self.pc_frames[..count].copy_from_slice(&pcs[..count]);
        self.pc_count = count as u32;
        self.heap = heap.unwrap_or_default();
        self.path = path;
        self.path_names = [CompactFrameName::EMPTY; MAX_FRAME_DEPTH];
        let name_count = names.len().min(path.len()).min(MAX_FRAME_DEPTH);
        self.path_names[..name_count].copy_from_slice(&names[..name_count]);
        self.generation = generation;

        compiler_fence(Ordering::SeqCst);
        self.state = STATE_TENTATIVE;
    }

    /// Layer-1 recovery caught the panic: the staged record is void.
    pub(crate) fn clear_tentative(&mut self) {
        if self.state == STATE_TENTATIVE {
            self.state = STATE_EMPTY;
        }
    }

    /// Unwinding failed (or a watchdog record is being synthesized): commit.
    pub(crate) fn finalize(&mut self) {
        compiler_fence(Ordering::SeqCst);
        self.state = STATE_FINAL;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(msg: &str) -> CrashRecord {
        let mut rec = CrashRecord::EMPTY;
        let mut path = FramePath::EMPTY;
        path.push(FrameKind::Boot, 1);
        path.push(FrameKind::NodeRender, 2);
        let mut names = [CompactFrameName::EMPTY; 2];
        names[0].set(FrameKind::Boot as u8, "boot");
        names[1].set(FrameKind::NodeRender as u8, "nodes/fire");
        rec.stage(
            CrashCause::Panic,
            &msg,
            Some(("src/thing.rs", 42)),
            &[0x4200_0000, 0x4200_0004],
            None,
            path,
            &names,
            7,
        );
        rec
    }

    #[test]
    fn stage_records_all_fields_tentatively() {
        let rec = staged("index out of bounds");
        assert!(rec.is_tentative());
        assert!(!rec.is_final());
        assert_eq!(rec.cause(), CrashCause::Panic);
        assert_eq!(
            rec.msg().as_str(),
            "index out of bounds (at src/thing.rs:42)"
        );
        assert_eq!(rec.pc_frames(), &[0x4200_0000, 0x4200_0004]);
        assert_eq!(rec.generation(), 7);
        assert_eq!(rec.path().len(), 2);
        assert_eq!(rec.path_names()[1].name(), "nodes/fire");
    }

    #[test]
    fn clear_tentative_voids_staged_but_not_final() {
        let mut rec = staged("x");
        rec.clear_tentative();
        assert!(rec.is_empty());

        let mut rec = staged("x");
        rec.finalize();
        rec.clear_tentative();
        assert!(rec.is_final());
    }

    #[test]
    fn long_message_truncates_at_capacity() {
        let long = "m".repeat(CRASH_MSG_CAP * 2);
        let rec = staged(&long);
        assert_eq!(rec.msg().as_str().len(), CRASH_MSG_CAP);
    }

    #[test]
    fn oom_heap_stats_survive() {
        let mut rec = CrashRecord::EMPTY;
        rec.stage(
            CrashCause::Oom,
            &"memory allocation of 65536 bytes failed",
            None,
            &[],
            Some(OomStats {
                requested: 65536,
                align: 4,
                free: 12000,
                used: 288000,
            }),
            FramePath::EMPTY,
            &[],
            3,
        );
        assert_eq!(rec.heap().requested, 65536);
        assert_eq!(rec.heap().free, 12000);
        assert_eq!(rec.cause(), CrashCause::Oom);
    }
}
