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
//! 56  self       u32        this table's own address
//! 60  count      u32        number of entries
//! 64  pending    u32        address of the wake pending word, 0 = no wake
//! 68  reserved   u32        0
//! 72  entries    [entry; count], 12 bytes each:
//!       +0 id u16, +2 kind u8, +3 shape u8,
//!       +4 function u32 (seam function address),
//!       +8 engaged  u32 (engaged-byte address, 0 = none)
//! ```
//!
//! `self` is what lets an emulator tell a **live** table from a stale one:
//! after a firmware update a flash chip can hold two cores, so two tables. A
//! table is live when the running cache MMU translates its own address back
//! to the flash offset it was scanned at.
//!
//! The firmware builds it with [`SeamTable`] (32-bit targets: the layout is
//! asserted there). The emulator, a 64-bit host, never casts bytes to these
//! types; it reads the offsets above through [`TableView`].

use crate::{SeamDecl, SeamKind, SeamShape};

/// The 16 bytes the emulator scans the flash image for. Non-ASCII at both
/// ends so no string in the image can be it by accident.
pub const MAGIC: [u8; 16] = *b"\xa5LPSEAM-TABLE\x5a\xc3\x3c";

pub const OFFSET_ABI: usize = 16;
pub const OFFSET_VERSION: usize = 24;
pub const VERSION_LEN: usize = 32;
pub const OFFSET_SELF: usize = 56;
pub const OFFSET_COUNT: usize = 60;
pub const OFFSET_PENDING: usize = 64;
pub const OFFSET_RESERVED: usize = 68;
pub const OFFSET_ENTRIES: usize = 72;
pub const ENTRY_LEN: usize = 12;
/// A table claiming more entries than this is not a table.
pub const MAX_ENTRIES: u32 = 64;

/// An address in the firmware's own address space (`*const ()` so a static
/// can hold a function's, a byte's or its own address; `Sync` because it is
/// only ever read as a number).
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Addr(pub *const ());

// SAFETY: the pointer is never dereferenced through this type; it is data the
// emulator reads.
unsafe impl Sync for Addr {}

impl Addr {
    pub const NONE: Addr = Addr(core::ptr::null());

    /// The address of a static (the table naming itself, a pending word).
    pub const fn of<T>(item: &'static T) -> Addr {
        Addr(item as *const T as *const ())
    }
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
    pub const fn new(decl: &SeamDecl, function: Addr, engaged: Addr) -> Self {
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
    pub self_addr: Addr,
    pub count: u32,
    pub pending: Addr,
    pub reserved: u32,
    pub entries: [SeamEntry; N],
}

impl<const N: usize> SeamTable<N> {
    /// `self_addr` is the table's own address: a static may name itself,
    /// `SeamTable::new(v, Addr::of(&TABLE), …)` inside `static TABLE`'s own
    /// initialiser.
    pub const fn new(
        version: &str,
        self_addr: Addr,
        pending: Addr,
        entries: [SeamEntry; N],
    ) -> Self {
        Self {
            magic: MAGIC,
            abi: crate::SEAM_ABI_ID,
            version: pad_version(version),
            self_addr,
            count: N as u32,
            pending,
            reserved: 0,
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
    assert!(core::mem::offset_of!(SeamTable<1>, self_addr) == OFFSET_SELF);
    assert!(core::mem::offset_of!(SeamTable<1>, count) == OFFSET_COUNT);
    assert!(core::mem::offset_of!(SeamTable<1>, pending) == OFFSET_PENDING);
    assert!(core::mem::offset_of!(SeamTable<1>, reserved) == OFFSET_RESERVED);
    assert!(core::mem::offset_of!(SeamTable<1>, entries) == OFFSET_ENTRIES);
};

// ---- the reader's side ------------------------------------------------------

/// Every offset in `image` where [`MAGIC`] starts, on a 4-byte boundary (the
/// table is 8-aligned in `.rodata`, and every image format the emulator scans
/// keeps a segment's flash offset 4-aligned).
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

    /// The table's own address, as the firmware linked it.
    pub fn self_addr(&self) -> u32 {
        u32_at(self.bytes, OFFSET_SELF).unwrap_or(0)
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
        assert_eq!(view.self_addr(), 0x4202_0000);
        assert_eq!(view.pending(), 0x4080_1000);
        let e = view.entries().next().unwrap();
        assert_eq!(e.id, 1);
        assert_eq!(e.kind, Some(SeamKind::Performance));
        assert_eq!(e.shape, Some(SeamShape::Replace));
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
        // Not even the count is read: an absurd one behind a mismatch is not
        // an error, because a mismatch's layout is not ours to judge.
        let mut t = t;
        t[OFFSET_COUNT..OFFSET_COUNT + 4].copy_from_slice(&1000u32.to_le_bytes());
        assert!(matches!(read(&t, 0), Ok(Read::Mismatch { .. })));
        assert!(matches!(read(&t[..24], 0), Ok(Read::Mismatch { .. })));
    }

    #[test]
    fn a_truncated_or_absurd_table_is_refused() {
        let t = table_bytes(crate::SEAM_ABI_ID, &[(1, 1, 1, 0, 0)]);
        assert_eq!(read(&t[..20], 0).unwrap_err(), ReadError::Truncated);
        assert_eq!(
            read(&t[..OFFSET_ENTRIES + ENTRY_LEN - 1], 0).unwrap_err(),
            ReadError::Truncated
        );
        let mut t = t;
        t[OFFSET_COUNT..OFFSET_COUNT + 4].copy_from_slice(&1000u32.to_le_bytes());
        assert_eq!(read(&t, 0).unwrap_err(), ReadError::TooManyEntries(1000));
    }

    #[test]
    fn a_static_table_may_name_its_own_address() {
        static TABLE: SeamTable<1> = SeamTable::new(
            "v1",
            Addr::of(&TABLE),
            Addr::NONE,
            [SeamEntry::new(
                &crate::ws281x_wait_step::DECL,
                Addr::NONE,
                Addr::NONE,
            )],
        );
        assert_eq!(TABLE.self_addr.0, &TABLE as *const _ as *const ());
        assert_eq!(TABLE.count, 1);
        assert_eq!(TABLE.abi, crate::SEAM_ABI_ID);
        assert_eq!(TABLE.entries[0].id, crate::ws281x_wait_step::ID);
    }

    fn table_bytes(abi: u64, entries: &[(u16, u8, u8, u32, u32)]) -> [u8; 256] {
        let mut b = [0u8; 256];
        b[..16].copy_from_slice(&MAGIC);
        b[16..24].copy_from_slice(&abi.to_le_bytes());
        b[24..30].copy_from_slice(b"abc123");
        b[OFFSET_SELF..OFFSET_SELF + 4].copy_from_slice(&0x4202_0000u32.to_le_bytes());
        b[OFFSET_COUNT..OFFSET_COUNT + 4].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        b[OFFSET_PENDING..OFFSET_PENDING + 4].copy_from_slice(&0x4080_1000u32.to_le_bytes());
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
}
