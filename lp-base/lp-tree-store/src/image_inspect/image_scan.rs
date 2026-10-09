//! Reading a raw image sector by sector, the way mount's first pass does
//! (`mount` + `record_scan.rs`), but keeping every record it looks at and
//! never writing: classify each header (FORMAT.md "Sector"), then read a
//! trusted sector's records until the end, or the first one that does not
//! check.

use alloc::vec::Vec;

use lp_crc32::crc32;

use super::image_report::{
    HeadReport, HeaderReport, RecordKindReport, RecordReport, RecordStatus, SectorReport,
    SectorState,
};
use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader};
use crate::record_kind::RecordKind;
use crate::root_record::RootRecord;
use crate::sector_header::{
    FORMAT_VERSION, HeadKind, SECTOR_HEADER_LEN, SECTOR_MAGIC, SectorHeader, SectorRead,
};

/// The sector sizes the format allows (FORMAT.md "The medium").
const SIZES: [u32; 7] = [512, 1024, 2048, 4096, 8192, 16384, 32768];

/// The sector size an image's own headers give: the size whose stride finds
/// the most version-3 headers that check and name that size. `None` when no
/// header does (an image that is blank, killed or all newer).
pub fn detect_sector_size(image: &[u8]) -> Option<u32> {
    let mut best: Option<(usize, u32)> = None;
    for &size in &SIZES {
        let mut hits = 0;
        let mut at = 0usize;
        while at + SECTOR_HEADER_LEN as usize <= image.len() {
            let h = &image[at..at + SECTOR_HEADER_LEN as usize];
            let v3 = u32::from_le_bytes([h[0], h[1], h[2], h[3]]) == SECTOR_MAGIC
                && u16::from_le_bytes([h[4], h[5]]) == FORMAT_VERSION
                && u32::from_le_bytes([h[20], h[21], h[22], h[23]]) == crc32(&h[..20])
                && u32::from(h[7]) == size.trailing_zeros();
            hits += usize::from(v3);
            at += size as usize;
        }
        if hits > best.map_or(0, |b| b.0) {
            best = Some((hits, size));
        }
    }
    best.map(|b| b.1)
}

/// One sector as scanned: the report plus nothing else; the closure pass
/// fills in `status`, `retired` and the live/garbage bytes.
pub fn scan_sector(image: &[u8], index: u32, size: u32) -> SectorReport {
    let start = index as usize * size as usize;
    let bytes = &image[start..start + size as usize];
    let mut report = SectorReport {
        index,
        offset: start as u64,
        state: SectorState::Blank,
        retired: false,
        header: None,
        records: Vec::new(),
        records_end: SECTOR_HEADER_LEN,
        closed: None,
        tail_erased: false,
        live_bytes: 0,
        garbage_bytes: 0,
    };
    let mut head = [0u8; SECTOR_HEADER_LEN as usize];
    head.copy_from_slice(&bytes[..SECTOR_HEADER_LEN as usize]);
    report.state = classify(&head, bytes, size);
    let SectorState::Valid = report.state else {
        return report;
    };
    let SectorRead::Trusted { header, appendable } = SectorHeader::decode(&head, size) else {
        return report;
    };
    report.header = Some(HeaderReport {
        version: FORMAT_VERSION,
        head: match header.kind {
            HeadKind::Cold => HeadReport::Cold,
            HeadKind::Hot => HeadReport::Hot,
        },
        seq: header.seq,
        erase_count: header.erase_count,
        compat_flags: u16::from_le_bytes([head[8], head[9]]),
        incompat_flags: u16::from_le_bytes([head[10], head[11]]),
        appendable,
    });
    scan_records(bytes, size, &mut report);
    report
}

fn classify(head: &[u8; SECTOR_HEADER_LEN as usize], sector: &[u8], size: u32) -> SectorState {
    let magic = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) == SECTOR_MAGIC;
    let version = u16::from_le_bytes([head[4], head[5]]);
    if magic && version > FORMAT_VERSION {
        return SectorState::Newer { version };
    }
    match SectorHeader::decode(head, size) {
        SectorRead::Trusted { .. } => SectorState::Valid,
        SectorRead::Unsupported(why) => SectorState::Unsupported { why },
        SectorRead::Untrusted => {
            if sector.iter().all(|&b| b == 0xFF) {
                SectorState::Blank
            } else if head.iter().all(|&b| b == 0) {
                SectorState::Killed
            } else if head.iter().all(|&b| b == 0xFF) {
                SectorState::NeedsErase {
                    why: "erased header over data",
                }
            } else if magic && version != FORMAT_VERSION {
                SectorState::NeedsErase {
                    why: "header of an older format version",
                }
            } else if magic {
                SectorState::NeedsErase {
                    why: "header fails its CRC",
                }
            } else {
                SectorState::NeedsErase {
                    why: "no header magic",
                }
            }
        }
    }
}

/// A trusted sector's records, as `record_scan::scan_sector` reads them:
/// stop at the erased end, or at the first record that does not check (its
/// header has id 0, it runs past the sector, its CRC fails, or it is a root
/// that does not decode).
fn scan_records(bytes: &[u8], size: u32, out: &mut SectorReport) {
    let mut off = SECTOR_HEADER_LEN;
    while off + RECORD_HEADER_LEN <= size {
        let raw: &[u8; RECORD_HEADER_LEN as usize] = bytes
            [off as usize..(off + RECORD_HEADER_LEN) as usize]
            .try_into()
            .expect("16 bytes");
        let id = u64::from_le_bytes(raw[4..12].try_into().expect("8 bytes"));
        let len = u16::from_le_bytes([raw[2], raw[3]]);
        let parsed = RecordHeader::parse(raw);
        let (kind, root) = match parsed {
            HeaderRead::End => break,
            HeaderRead::Bad => {
                out.closed = Some("record header with id 0");
                out.records.push(untrusted(off, raw, id, false));
                break;
            }
            HeaderRead::Unknown { .. } => (RecordKindReport::Unknown { kind: raw[0] }, false),
            HeaderRead::Record(r) => (
                match r.kind {
                    RecordKind::Blob => RecordKindReport::Blob,
                    RecordKind::Multi => RecordKindReport::Multi,
                    RecordKind::Dir => RecordKindReport::Dir,
                    RecordKind::Root => RecordKindReport::Root,
                },
                r.kind == RecordKind::Root,
            ),
        };
        let total = RECORD_HEADER_LEN + u32::from(len);
        if off + total > size {
            out.closed = Some("record runs past the end of the sector");
            out.records.push(untrusted(off, raw, id, false));
            break;
        }
        let payload = &bytes[(off + RECORD_HEADER_LEN) as usize..(off + total) as usize];
        if !RecordHeader::crc_ok(raw, payload) {
            out.closed = Some("record fails its CRC (a torn write)");
            out.records.push(untrusted(off, raw, id, false));
            break;
        }
        let mut root_seq = None;
        if root {
            match RootRecord::decode(payload) {
                Some(r) => root_seq = Some(r.seq),
                None => {
                    out.closed = Some("root record does not decode");
                    out.records.push(untrusted(off, raw, id, true));
                    break;
                }
            }
        }
        out.records.push(RecordReport {
            offset: off,
            kind,
            codec: raw[1],
            len,
            id,
            crc_ok: true,
            status: if matches!(kind, RecordKindReport::Unknown { .. }) {
                RecordStatus::Unknown
            } else {
                RecordStatus::Garbage
            },
            root_seq,
            problem: None,
        });
        off += total;
        out.records_end = off;
    }
    out.tail_erased = bytes[out.records_end as usize..].iter().all(|&b| b == 0xFF);
}

/// The record that closed a sector. `crc_ok` is true only for a root whose
/// CRC was good but whose payload does not decode.
fn untrusted(
    offset: u32,
    raw: &[u8; RECORD_HEADER_LEN as usize],
    id: u64,
    crc_ok: bool,
) -> RecordReport {
    let kind = match raw[0] {
        1 => RecordKindReport::Blob,
        2 => RecordKindReport::Multi,
        3 => RecordKindReport::Dir,
        4 => RecordKindReport::Root,
        other => RecordKindReport::Unknown { kind: other },
    };
    RecordReport {
        offset,
        kind,
        codec: raw[1],
        len: u16::from_le_bytes([raw[2], raw[3]]),
        id,
        crc_ok,
        status: RecordStatus::Untrusted,
        root_seq: None,
        problem: Some(if crc_ok {
            "root payload does not decode"
        } else {
            "does not check"
        }),
    }
}
