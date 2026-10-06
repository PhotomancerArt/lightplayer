//! Find the firmware's seam tables in a flash image.
//!
//! The scan reads the **flash chip's bytes** — a merged 4 MiB image, a
//! `kind=rom-up` flash file, what a direct load staged, or what Studio's
//! Update firmware wrote — and never an ELF, because the Studio tab has
//! none. It costs one pass over the chip at 4-byte steps, once per chip
//! start, and only when a seam was asked for.
//!
//! **Every** magic hit is reported. After an update a chip can hold two
//! cores, so two tables; which one is live is the MMU's to say
//! ([`super::seam_resolution`]), never the scan's.

use std::fmt;

use lp_seam::table::{self, Read};
use lp_seam::{SeamKind, SeamShape};

/// One entry of a table whose identity matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScannedEntry {
    pub id: u16,
    pub kind: Option<SeamKind>,
    pub shape: Option<SeamShape>,
    /// The seam function's address in the firmware's address space.
    pub function: u32,
    /// The engaged byte's address, 0 when the entry has none.
    pub engaged: u32,
}

/// A table whose identity matched this emulator's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Where the magic starts, as a flash offset.
    pub offset: u32,
    pub abi: u64,
    /// The table's own address, as the firmware linked it.
    pub self_addr: u32,
    pub version: String,
    /// The wake pending word's address, 0 = this image has no wake.
    pub pending: u32,
    pub entries: Vec<ScannedEntry>,
}

impl Candidate {
    pub fn entry(&self, id: u16) -> Option<&ScannedEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// One magic hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanHit {
    Candidate(Candidate),
    /// A table built from different seam declarations. Nothing past its
    /// identity is read (the layout after it is the identity's to define).
    Mismatch {
        offset: u32,
        abi: u64,
    },
}

/// Everything the scan found, in flash order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanResult {
    pub hits: Vec<ScanHit>,
}

impl ScanResult {
    pub fn candidates(&self) -> impl Iterator<Item = &Candidate> {
        self.hits.iter().filter_map(|h| match h {
            ScanHit::Candidate(c) => Some(c),
            ScanHit::Mismatch { .. } => None,
        })
    }

    pub fn mismatches(&self) -> impl Iterator<Item = (u32, u64)> + '_ {
        self.hits.iter().filter_map(|h| match h {
            ScanHit::Mismatch { offset, abi } => Some((*offset, *abi)),
            ScanHit::Candidate(_) => None,
        })
    }

    /// Why this image cannot engage anything at all, in one phrase, or
    /// `None` when it holds at least one candidate.
    pub fn why_none(&self) -> Option<String> {
        if self.candidates().next().is_some() {
            return None;
        }
        let ours = lp_seam::SEAM_ABI_ID;
        Some(match self.mismatches().next() {
            None => format!("no seam table in the image (emulator abi {ours:016x})"),
            Some((offset, abi)) => format!(
                "the image's seam table (flash {offset:#x}) has abi {abi:016x}, this emulator has \
                 {ours:016x}: the firmware was built from different seam declarations"
            ),
        })
    }
}

/// Scan `flash` for seam tables.
pub fn scan(flash: &[u8]) -> ScanResult {
    let mut hits = Vec::new();
    for at in table::find_magic(flash) {
        match table::read(flash, at) {
            Ok(Read::Match(view)) => hits.push(ScanHit::Candidate(Candidate {
                offset: at as u32,
                abi: view.abi(),
                self_addr: view.self_addr(),
                version: view.version().to_string(),
                pending: view.pending(),
                entries: view
                    .entries()
                    .map(|e| ScannedEntry {
                        id: e.id,
                        kind: e.kind,
                        shape: e.shape,
                        function: e.function,
                        engaged: e.engaged,
                    })
                    .collect(),
            })),
            Ok(Read::Mismatch { abi }) => hits.push(ScanHit::Mismatch {
                offset: at as u32,
                abi,
            }),
            // A magic followed by nonsense is not a table.
            Err(_) => {}
        }
    }
    ScanResult { hits }
}

impl fmt::Display for ScanResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(why) = self.why_none()
            && self.hits.len() <= 1
        {
            return f.write_str(&why);
        }
        let mut first = true;
        for hit in &self.hits {
            if !first {
                writeln!(f)?;
            }
            first = false;
            match hit {
                ScanHit::Mismatch { offset, abi } => write!(
                    f,
                    "seam table at flash {offset:#x}: abi {abi:016x} (not this emulator's \
                     {:016x}; not read further)",
                    lp_seam::SEAM_ABI_ID
                )?,
                ScanHit::Candidate(c) => {
                    write!(
                        f,
                        "seam table at flash {:#x}: abi {:016x}, firmware {}, self {:#010x}, \
                         pending {:#010x}, {} entr{}",
                        c.offset,
                        c.abi,
                        c.version,
                        c.self_addr,
                        c.pending,
                        c.entries.len(),
                        if c.entries.len() == 1 { "y" } else { "ies" }
                    )?;
                    for e in &c.entries {
                        let name = lp_seam::SeamDecl::by_id(e.id).map_or("?", |d| d.symbol);
                        write!(
                            f,
                            "\n  {:#06x} {name} {} {} fn={:#010x} engaged={:#010x}",
                            e.id,
                            e.kind.map_or("?", |k| k.as_str()),
                            e.shape.map_or("?", |s| s.as_str()),
                            e.function,
                            e.engaged
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_without_a_table_has_no_hits() {
        let r = scan(&vec![0xffu8; 4096]);
        assert!(r.hits.is_empty());
        assert!(r.why_none().unwrap().contains("no seam table"));
    }

    #[test]
    fn one_matching_table_is_one_candidate_with_its_entries() {
        let mut flash = vec![0xffu8; 64 * 1024];
        put_table(&mut flash, 0x4000, lp_seam::SEAM_ABI_ID, 0x4200_4000);
        let r = scan(&flash);
        let c: Vec<_> = r.candidates().collect();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].offset, 0x4000);
        assert_eq!(c[0].self_addr, 0x4200_4000);
        assert_eq!(c[0].version, "abcdef0");
        assert_eq!(c[0].entries[0].function, 0x4208_9d90);
        assert_eq!(c[0].entries[0].kind, Some(SeamKind::Performance));
        assert!(r.why_none().is_none());
    }

    #[test]
    fn two_matching_tables_are_two_candidates_not_a_refusal() {
        let mut flash = vec![0xffu8; 64 * 1024];
        put_table(&mut flash, 0x4000, lp_seam::SEAM_ABI_ID, 0x4200_4000);
        put_table(&mut flash, 0x9000, lp_seam::SEAM_ABI_ID, 0x4200_4000);
        let r = scan(&flash);
        let offsets: Vec<u32> = r.candidates().map(|c| c.offset).collect();
        assert_eq!(offsets, [0x4000, 0x9000]);
    }

    #[test]
    fn a_mismatch_beside_a_match_is_reported_and_not_read() {
        let mut flash = vec![0xffu8; 64 * 1024];
        put_table(&mut flash, 0x4000, lp_seam::SEAM_ABI_ID ^ 0x55, 0x4200_4000);
        put_table(&mut flash, 0x8000, lp_seam::SEAM_ABI_ID, 0x4200_8000);
        let r = scan(&flash);
        assert_eq!(r.hits.len(), 2);
        assert_eq!(
            r.mismatches().collect::<Vec<_>>(),
            [(0x4000, lp_seam::SEAM_ABI_ID ^ 0x55)]
        );
        assert_eq!(r.candidates().count(), 1);
        // Mismatch only: the reason names both identities.
        let mut only = vec![0xffu8; 64 * 1024];
        put_table(&mut only, 0x4000, lp_seam::SEAM_ABI_ID ^ 0x55, 0x4200_4000);
        let why = scan(&only).why_none().unwrap();
        assert!(why.contains("different seam declarations"), "{why}");
    }

    fn put_table(flash: &mut [u8], at: usize, abi: u64, self_addr: u32) {
        use lp_seam::table::*;
        flash[at..at + 16].copy_from_slice(&MAGIC);
        flash[at + OFFSET_ABI..at + OFFSET_ABI + 8].copy_from_slice(&abi.to_le_bytes());
        flash[at + OFFSET_VERSION..at + OFFSET_VERSION + VERSION_LEN].fill(0);
        flash[at + OFFSET_VERSION..at + OFFSET_VERSION + 7].copy_from_slice(b"abcdef0");
        flash[at + OFFSET_SELF..at + OFFSET_SELF + 4].copy_from_slice(&self_addr.to_le_bytes());
        flash[at + OFFSET_COUNT..at + OFFSET_COUNT + 4].copy_from_slice(&1u32.to_le_bytes());
        flash[at + OFFSET_PENDING..at + OFFSET_RESERVED + 4].fill(0);
        let e = at + OFFSET_ENTRIES;
        flash[e..e + 2].copy_from_slice(&1u16.to_le_bytes());
        flash[e + 2] = 1;
        flash[e + 3] = 1;
        flash[e + 4..e + 8].copy_from_slice(&0x4208_9d90u32.to_le_bytes());
        flash[e + 8..e + 12].copy_from_slice(&0u32.to_le_bytes());
    }
}
