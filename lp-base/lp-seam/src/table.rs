//! The descriptor table: where it is, and what is in it.
//!
//! **Only [`MAGIC`] (offset 0) and the identity (offset 16, a little-endian
//! `u64`) are at fixed offsets.** Everything after them is defined by the
//! identity: a reader whose [`crate::SEAM_ABI_ID`] differs from the table's
//! must not read further. This file is that definition for the current
//! declarations.
//!
//! ```text
//!  0  magic      [u8; 16]
//! 16  abi        u64        SEAM_ABI_ID of the firmware's declarations
//! 24  version    [u8; 32]   LP_APP_VERSION, NUL-padded (diagnostics only)
//! 56  count      u32        number of entries
//! 60  pending    u32        address of the wake pending word, 0 = no wake
//! 64  entries    [entry; count], 12 bytes each:
//!       +0 id u16, +2 kind u8, +3 shape u8,
//!       +4 function u32 (seam function address),
//!       +8 engaged  u32 (engaged-byte address, 0 = unused)
//! ```
//!
//! The firmware builds it with [`SeamTable`] (32-bit targets: the layout is
//! asserted there). The emulator, a 64-bit host, never casts bytes to these
//! types; it reads the offsets above through [`TableView`].

use crate::{SeamKind, SeamShape};

/// The 16 bytes the emulator scans the flash image for. Non-ASCII at both
/// ends so no string in the image can be it by accident.
pub const MAGIC: [u8; 16] = *b"\xa5LPSEAM-TABLE\x5a\xc3\x3c";

pub const OFFSET_ABI: usize = 16;
pub const OFFSET_VERSION: usize = 24;
pub const VERSION_LEN: usize = 32;
pub const OFFSET_COUNT: usize = 56;
pub const OFFSET_PENDING: usize = 60;
pub const OFFSET_ENTRIES: usize = 64;
pub const ENTRY_LEN: usize = 12;
/// A table claiming more entries than this is not a table.
pub const MAX_ENTRIES: u32 = 64;

/// An address in the firmware's own address space (`*const ()` so a static
/// can hold a function or a byte's address; `Sync` because it is only ever
/// read as a number).
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Addr(pub *const ());

// SAFETY: the pointer is never dereferenced through this type; it is data the
// emulator reads.
unsafe impl Sync for Addr {}

impl Addr {
    pub const NONE: Addr = Addr(core::ptr::null());
}

/// One table entry, as the firmware lays it out.
#[repr(C)]
pub struct SeamEntry {
    pub id: u16,
    pub kind: u8,
    pub shape: u8,
    pub function: Addr,
    pub engaged: Addr,
}

impl SeamEntry {
    pub const fn new(decl: &crate::SeamDecl, function: Addr, engaged: Addr) -> Self {
        Self {
            id: decl.id,
            kind: decl.kind as u8,
            shape: decl.shape as u8,
            function,
            engaged,
        }
    }
}

/// The whole table, as the firmware lays it out.
#[repr(C, align(8))]
pub struct SeamTable<const N: usize> {
    pub magic: [u8; 16],
    pub abi: u64,
    pub version: [u8; VERSION_LEN],
    pub count: u32,
    pub pending: Addr,
    pub entries: [SeamEntry; N],
}

impl<const N: usize> SeamTable<N> {
    pub const fn new(version: &str, pending: Addr, entries: [SeamEntry; N]) -> Self {
        Self {
            magic: MAGIC,
            abi: crate::SEAM_ABI_ID,
            version: pad_version(version),
            count: N as u32,
            pending,
            entries,
        }
    }
}

const fn pad_version(version: &str) -> [u8; VERSION_LEN] {
    let mut out = [0u8; VERSION_LEN];
    let bytes = version.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i < VERSION_LEN {
        out[i] = bytes[i];
        i += 1;
    }
    out
}

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(core::mem::size_of::<SeamEntry>() == ENTRY_LEN);
    assert!(core::mem::offset_of!(SeamTable<1>, abi) == OFFSET_ABI);
    assert!(core::mem::offset_of!(SeamTable<1>, version) == OFFSET_VERSION);
    assert!(core::mem::offset_of!(SeamTable<1>, count) == OFFSET_COUNT);
    assert!(core::mem::offset_of!(SeamTable<1>, pending) == OFFSET_PENDING);
    assert!(core::mem::offset_of!(SeamTable<1>, entries) == OFFSET_ENTRIES);
};

// ---- the reader's side ------------------------------------------------------

/// Every offset in `image` where [`MAGIC`] starts, on a 4-byte boundary (the
/// table is 8-aligned in `.rodata`, and every image format the emulator scans
/// keeps the app's flash offset 4-aligned).
pub fn find_magic(image: &[u8]) -> impl Iterator<Item = usize> + '_ {
    (0..image.len().saturating_sub(MAGIC.len() - 1))
        .step_by(4)
        .filter(move |&at| image[at..at + MAGIC.len()] == MAGIC)
}

/// What reading a table at a magic hit found.
#[derive(Clone, Copy, Debug)]
pub enum Read<'a> {
    /// Same declarations as this build: the rest of the table is readable.
    Match(TableView<'a>),
    /// Different declarations. Nothing past the identity may be read.
    Mismatch { abi: u64 },
}

/// Why bytes at a magic hit are not a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    Truncated,
    TooManyEntries(u32),
}

/// Read the table whose magic starts at `at`.
pub fn read(image: &[u8], at: usize) -> Result<Read<'_>, ReadError> {
    let bytes = image.get(at..).ok_or(ReadError::Truncated)?;
    let abi = u64_at(bytes, OFFSET_ABI).ok_or(ReadError::Truncated)?;
    if abi != crate::SEAM_ABI_ID {
        return Ok(Read::Mismatch { abi });
    }
    let count = u32_at(bytes, OFFSET_COUNT).ok_or(ReadError::Truncated)?;
    if count > MAX_ENTRIES {
        return Err(ReadError::TooManyEntries(count));
    }
    let len = OFFSET_ENTRIES + count as usize * ENTRY_LEN;
    let bytes = bytes.get(..len).ok_or(ReadError::Truncated)?;
    Ok(Read::Match(TableView { bytes }))
}

/// A table this build can read (its identity matched).
#[derive(Clone, Copy, Debug)]
pub struct TableView<'a> {
    bytes: &'a [u8],
}

/// One entry of a [`TableView`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryView {
    pub id: u16,
    pub kind: Option<SeamKind>,
    pub shape: Option<SeamShape>,
    pub function: u32,
    pub engaged: u32,
}

impl<'a> TableView<'a> {
    pub fn abi(&self) -> u64 {
        u64_at(self.bytes, OFFSET_ABI).unwrap_or(0)
    }

    /// The firmware's `LP_APP_VERSION`, for diagnostics.
    pub fn version(&self) -> &'a str {
        let raw = &self.bytes[OFFSET_VERSION..OFFSET_VERSION + VERSION_LEN];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(VERSION_LEN);
        core::str::from_utf8(&raw[..end]).unwrap_or("<not utf-8>")
    }

    pub fn pending(&self) -> u32 {
        u32_at(self.bytes, OFFSET_PENDING).unwrap_or(0)
    }

    pub fn entries(&self) -> impl Iterator<Item = EntryView> + 'a {
        let bytes = self.bytes;
        bytes[OFFSET_ENTRIES..]
            .chunks_exact(ENTRY_LEN)
            .map(|e| EntryView {
                id: u16::from_le_bytes([e[0], e[1]]),
                kind: SeamKind::from_u8(e[2]),
                shape: SeamShape::from_u8(e[3]),
                function: u32::from_le_bytes([e[4], e[5], e[6], e[7]]),
                engaged: u32::from_le_bytes([e[8], e[9], e[10], e[11]]),
            })
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_bytes(abi: u64, entries: &[(u16, u8, u8, u32, u32)]) -> [u8; 256] {
        let mut b = [0u8; 256];
        b[..16].copy_from_slice(&MAGIC);
        b[16..24].copy_from_slice(&abi.to_le_bytes());
        b[24..30].copy_from_slice(b"abc123");
        b[56..60].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        b[60..64].copy_from_slice(&0x4080_1000u32.to_le_bytes());
        for (i, (id, kind, shape, f, e)) in entries.iter().enumerate() {
            let at = OFFSET_ENTRIES + i * ENTRY_LEN;
            b[at..at + 2].copy_from_slice(&id.to_le_bytes());
            b[at + 2] = *kind;
            b[at + 3] = *shape;
            b[at + 4..at + 8].copy_from_slice(&f.to_le_bytes());
            b[at + 8..at + 12].copy_from_slice(&e.to_le_bytes());
        }
        b
    }

    #[test]
    fn a_matching_table_is_read_entry_by_entry() {
        let mut image = [0xffu8; 1024];
        let t = table_bytes(crate::SEAM_ABI_ID, &[(1, 1, 1, 0x4200_1234, 0)]);
        image[512..768].copy_from_slice(&t);
        let hits: [usize; 1] = [512];
        assert!(find_magic(&image).eq(hits.iter().copied()));
        let Read::Match(view) = read(&image, 512).unwrap() else {
            panic!("expected a match");
        };
        assert_eq!(view.version(), "abc123");
        assert_eq!(view.pending(), 0x4080_1000);
        let e = view.entries().next().unwrap();
        assert_eq!(e.id, 1);
        assert_eq!(e.kind, Some(SeamKind::Performance));
        assert_eq!(e.function, 0x4200_1234);
        assert_eq!(view.entries().count(), 1);
    }

    #[test]
    fn a_table_from_other_declarations_is_a_mismatch_and_nothing_more() {
        let t = table_bytes(crate::SEAM_ABI_ID ^ 1, &[(1, 1, 1, 0x4200_1234, 0)]);
        match read(&t, 0).unwrap() {
            Read::Mismatch { abi } => assert_eq!(abi, crate::SEAM_ABI_ID ^ 1),
            Read::Match(_) => panic!("must not match"),
        }
    }

    #[test]
    fn a_truncated_or_absurd_table_is_refused() {
        let t = table_bytes(crate::SEAM_ABI_ID, &[]);
        assert_eq!(read(&t[..20], 0).unwrap_err(), ReadError::Truncated);
        let mut t = t;
        t[56..60].copy_from_slice(&1000u32.to_le_bytes());
        assert_eq!(read(&t, 0).unwrap_err(), ReadError::TooManyEntries(1000));
    }
}
