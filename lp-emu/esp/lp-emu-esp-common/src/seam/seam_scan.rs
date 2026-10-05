//! Find the firmware's seam table in a flash image.
//!
//! The scan reads the **flash chip's bytes** — a merged 4 MiB image, a
//! `kind=rom-up` flash file, or what a direct load staged — and never an
//! ELF, because the Studio tab has none. It costs one pass over the chip at
//! 4-byte steps, once per run, and only when a seam was asked for.

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
    /// The engaged byte's address, 0 when unused.
    pub engaged: u32,
}

/// A table whose identity matched this emulator's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedTable {
    /// Where the magic starts, as a flash offset.
    pub offset: u32,
    pub abi: u64,
    pub version: String,
    pub pending: u32,
    pub entries: Vec<ScannedEntry>,
}

/// What the scan found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanOutcome {
    /// No magic anywhere: an image from before seams (main's), or not a
    /// LightPlayer image.
    NoTable,
    /// A table built from different seam declarations. Nothing past its
    /// identity is read (the layout after it is the identity's to define).
    Mismatch { offset: u32, abi: u64 },
    Found(ScannedTable),
    /// More than one readable table: refuse to guess.
    Ambiguous(Vec<u32>),
}

impl ScanOutcome {
    pub fn table(&self) -> Option<&ScannedTable> {
        match self {
            ScanOutcome::Found(t) => Some(t),
            _ => None,
        }
    }
}

/// Scan `flash` for the seam table.
pub fn scan(flash: &[u8]) -> ScanOutcome {
    let mut found: Vec<ScanOutcome> = Vec::new();
    for at in table::find_magic(flash) {
        match table::read(flash, at) {
            Ok(Read::Match(view)) => found.push(ScanOutcome::Found(ScannedTable {
                offset: at as u32,
                abi: view.abi(),
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
            Ok(Read::Mismatch { abi }) => found.push(ScanOutcome::Mismatch {
                offset: at as u32,
                abi,
            }),
            // A magic followed by nonsense is not a table.
            Err(_) => {}
        }
    }
    match found.len() {
        0 => ScanOutcome::NoTable,
        1 => found.pop().unwrap(),
        _ => ScanOutcome::Ambiguous(
            found
                .iter()
                .map(|f| match f {
                    ScanOutcome::Found(t) => t.offset,
                    ScanOutcome::Mismatch { offset, .. } => *offset,
                    _ => 0,
                })
                .collect(),
        ),
    }
}

impl fmt::Display for ScanOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ours = lp_seam::SEAM_ABI_ID;
        match self {
            ScanOutcome::NoTable => write!(f, "no seam table in the image (emulator abi {ours:016x})"),
            ScanOutcome::Mismatch { offset, abi } => write!(
                f,
                "seam table at flash {offset:#x} has abi {abi:016x}, this emulator has \
                 {ours:016x}: built from different seam declarations"
            ),
            ScanOutcome::Found(t) => {
                write!(
                    f,
                    "seam table at flash {:#x}: abi {:016x}, firmware {}, {} entr{}",
                    t.offset,
                    t.abi,
                    t.version,
                    t.entries.len(),
                    if t.entries.len() == 1 { "y" } else { "ies" }
                )?;
                for e in &t.entries {
                    let name = lp_seam::SeamDecl::by_id(e.id).map_or("?", |d| d.symbol);
                    write!(
                        f,
                        "\n  {:#06x} {name} {} {:?} fn={:#010x} engaged={:#010x}",
                        e.id,
                        e.kind.map_or("?", |k| k.as_str()),
                        e.shape,
                        e.function,
                        e.engaged
                    )?;
                }
                Ok(())
            }
            ScanOutcome::Ambiguous(at) => {
                write!(f, "{} seam tables in the image, at {at:x?}: refusing to guess", at.len())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_with(abi: u64) -> Vec<u8> {
        let mut flash = vec![0xffu8; 64 * 1024];
        let at = 0x4000;
        flash[at..at + 16].copy_from_slice(&table::MAGIC);
        flash[at + 16..at + 24].copy_from_slice(&abi.to_le_bytes());
        flash[at + 24..at + 56].fill(0);
        flash[at + 24..at + 31].copy_from_slice(b"abcdef0");
        flash[at + 56..at + 60].copy_from_slice(&1u32.to_le_bytes());
        flash[at + 60..at + 64].copy_from_slice(&0u32.to_le_bytes());
        flash[at + 64..at + 66].copy_from_slice(&1u16.to_le_bytes());
        flash[at + 66] = 1;
        flash[at + 67] = 1;
        flash[at + 68..at + 72].copy_from_slice(&0x4208_9d90u32.to_le_bytes());
        flash[at + 72..at + 76].copy_from_slice(&0u32.to_le_bytes());
        flash
    }

    #[test]
    fn a_matching_table_is_found_with_its_entries() {
        let outcome = scan(&image_with(lp_seam::SEAM_ABI_ID));
        let t = outcome.table().expect("found");
        assert_eq!(t.offset, 0x4000);
        assert_eq!(t.version, "abcdef0");
        assert_eq!(t.entries[0].function, 0x4208_9d90);
        assert_eq!(t.entries[0].kind, Some(SeamKind::Performance));
    }

    #[test]
    fn a_table_from_other_declarations_is_a_mismatch() {
        let outcome = scan(&image_with(lp_seam::SEAM_ABI_ID.wrapping_add(1)));
        assert!(matches!(outcome, ScanOutcome::Mismatch { offset: 0x4000, .. }));
    }

    #[test]
    fn an_image_without_a_table_has_none() {
        assert_eq!(scan(&vec![0xffu8; 4096]), ScanOutcome::NoTable);
    }
}
