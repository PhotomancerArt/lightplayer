//! What the inspector says about an image: plain data, `Serialize` for
//! `--json`. Every id is rendered as 16 hex digits (a u64 does not survive
//! a JSON number).

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Serialize, Serializer};

/// A u64 id as `0123456789abcdef`.
pub fn ser_id<S: Serializer>(id: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.collect_str(&format_args!("{id:016x}"))
}

/// Where the sector size came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectorSizeFrom {
    /// The sector headers say it (FORMAT.md: byte 7 of a header that checks).
    Headers,
    /// The caller gave it.
    Given,
    /// No header said, none was given: 4096, the C6's.
    Assumed,
}

/// What a sector's first 24 bytes (and, for an erased one, the rest) say.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectorState {
    /// A trusted header: the sector's records are read.
    Valid,
    /// Every byte is `0xFF`: erased, free.
    Blank,
    /// A killed header (24 zero bytes): an erase was about to happen.
    Killed,
    /// Not a header of this version and not blank or killed: the writer
    /// erases it before it is used again.
    NeedsErase { why: &'static str },
    /// The magic and a format version above this tool's. The store refuses
    /// to mount (FORMAT.md "Sector", rule 0); nothing in it is read.
    Newer { version: u16 },
    /// A good header this version must not read past (an unknown incompat
    /// flag or head kind, or another sector size). The mount is refused.
    Unsupported { why: &'static str },
}

impl SectorState {
    /// One word for tables: `valid`, `blank`, `killed`, `needs-erase`,
    /// `NEWER`, `UNSUPPORTED`.
    pub fn label(&self) -> &'static str {
        match self {
            SectorState::Valid => "valid",
            SectorState::Blank => "blank",
            SectorState::Killed => "killed",
            SectorState::NeedsErase { .. } => "needs-erase",
            SectorState::Newer { .. } => "NEWER",
            SectorState::Unsupported { .. } => "UNSUPPORTED",
        }
    }
}

/// Which write head opened a sector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadReport {
    Cold,
    Hot,
}

/// A trusted sector header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct HeaderReport {
    pub version: u16,
    pub head: HeadReport,
    pub seq: u32,
    pub erase_count: u32,
    pub compat_flags: u16,
    pub incompat_flags: u16,
    /// False when a compat flag this version does not know is set: the
    /// sector is read but never appended to.
    pub appendable: bool,
}

/// A record's kind byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKindReport {
    Blob,
    Multi,
    Dir,
    Root,
    /// A kind (or kind and codec pair) this version does not define:
    /// garbage by rule, skipped (FORMAT.md "Unknown records").
    Unknown {
        kind: u8,
    },
}

impl RecordKindReport {
    pub fn label(&self) -> &'static str {
        match self {
            RecordKindReport::Blob => "blob",
            RecordKindReport::Multi => "multi",
            RecordKindReport::Dir => "dir",
            RecordKindReport::Root => "root",
            RecordKindReport::Unknown { .. } => "unknown",
        }
    }
}

/// What the committed state makes of a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordStatus {
    /// The kept copy of a record the chosen root reaches.
    Live,
    /// An older copy of a live id (a GC copy whose original is still on
    /// flash): the reader keeps the one in the newest sector.
    Copy,
    /// Nothing the chosen root reaches names it: reclaimed by GC.
    Garbage,
    /// A record of a kind this version does not know.
    Unknown,
    /// The record that closed its sector: it did not check (torn write, bad
    /// CRC, a header with id 0, a root that does not decode).
    Untrusted,
}

/// One record in a sector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecordReport {
    pub offset: u32,
    pub kind: RecordKindReport,
    /// 0 stored, 1 deflate (FORMAT.md); anything else is an unknown record.
    pub codec: u8,
    pub len: u16,
    #[serde(serialize_with = "ser_id")]
    pub id: u64,
    pub crc_ok: bool,
    pub status: RecordStatus,
    /// For a root: its commit sequence.
    pub root_seq: Option<u64>,
    /// Why the record is untrusted.
    pub problem: Option<&'static str>,
}

impl RecordReport {
    /// Header and payload bytes.
    pub fn total_len(&self) -> u32 {
        16 + u32::from(self.len)
    }
}

/// One sector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SectorReport {
    pub index: u32,
    pub offset: u64,
    pub state: SectorState,
    /// The chosen root lists the sector as retired: never opened, erased or
    /// collected again.
    pub retired: bool,
    pub header: Option<HeaderReport>,
    pub records: Vec<RecordReport>,
    /// Offset just past the last record that checked (24 for none).
    pub records_end: u32,
    /// Why reading stopped before the sector's end: the record that did not
    /// check closed it. `None` = it ran to the erased tail.
    pub closed: Option<&'static str>,
    /// Every byte after `records_end` is `0xFF` (a writer may append).
    pub tail_erased: bool,
    /// Record bytes (headers included) the chosen root reaches.
    pub live_bytes: u32,
    /// Record bytes it does not: garbage, older copies, unknown records.
    pub garbage_bytes: u32,
}

/// What became of a root candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootOutcome {
    /// Mount adopts it.
    Chosen,
    /// Its closure is incomplete or malformed, so mount passes it over.
    Unusable { why: &'static str },
    /// Not tried: mount looks at the two newest roots and no further.
    NotTried,
}

/// A root record that checks and decodes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RootReport {
    pub seq: u64,
    #[serde(serialize_with = "ser_id")]
    pub id: u64,
    pub sector: u32,
    pub offset: u32,
    #[serde(serialize_with = "ser_id")]
    pub cold_dir: u64,
    #[serde(serialize_with = "ser_id")]
    pub hot_dir: u64,
    pub retired: Vec<u16>,
    pub outcome: RootOutcome,
}

/// File or directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKindReport {
    File,
    Dir,
}

/// One entry of the committed tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TreeEntryReport {
    /// Absolute path, `/`-separated; a name that is not UTF-8 shows as
    /// `U+FFFD` here.
    pub path: String,
    pub kind: EntryKindReport,
    /// The directory entry's size (0 for a directory).
    pub size: u32,
    #[serde(serialize_with = "ser_id")]
    pub id: u64,
    /// Listed in the hot directory (a `…/.lp/panel.json` file).
    pub hot: bool,
    /// The writer's name rules, broken: `list` reports such a name as
    /// corrupt.
    pub name_problem: Option<&'static str>,
}

/// What the real mount would do with this image.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MountVerdict {
    /// Mount adopts `ImageReport::chosen`.
    Mounts,
    /// A header refuses the mount (`StoreError::Unsupported`): never format
    /// over it. `rest_is_complete_store` = the trusted sectors alone hold a
    /// complete store (root chosen from them), the recovery path.
    Refused {
        sectors: Vec<u32>,
        rest_is_complete_store: bool,
    },
    /// No store here: `StoreError::Corrupt`.
    NoStore { why: &'static str },
}

/// Everything the inspector reads out of an image.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImageReport {
    pub image_bytes: u64,
    pub sector_size: u32,
    pub sector_size_from: SectorSizeFrom,
    pub sector_count: u32,
    /// Bytes past the last whole sector (an image that is not a whole
    /// number of sectors).
    pub trailing_bytes: u32,
    pub sectors: Vec<SectorReport>,
    /// Every root that checks and decodes, newest first.
    pub roots: Vec<RootReport>,
    /// Index into `roots` of the one mount adopts.
    pub chosen: Option<usize>,
    pub mount: MountVerdict,
    /// The committed tree (the chosen root's), cold directory first.
    pub tree: Vec<TreeEntryReport>,
    pub live_bytes: u64,
    pub garbage_bytes: u64,
}

impl ImageReport {
    pub fn chosen_root(&self) -> Option<&RootReport> {
        self.chosen.and_then(|i| self.roots.get(i))
    }

    /// Sectors whose header refuses the mount.
    pub fn refusing_sectors(&self) -> Vec<u32> {
        self.sectors
            .iter()
            .filter(|s| {
                matches!(
                    s.state,
                    SectorState::Newer { .. } | SectorState::Unsupported { .. }
                )
            })
            .map(|s| s.index)
            .collect()
    }
}
