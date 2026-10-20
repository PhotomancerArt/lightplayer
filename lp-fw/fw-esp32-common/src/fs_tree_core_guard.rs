//! A tree-store board's update guard (plan
//! `2026-10-08-2339-tree-store-firmware-and-emulator`, D8): a core install
//! that does not mount the tree store is refused, because that core would
//! find no littlefs on `lpfs`, take the tree store for an unformatted
//! partition, and format it — every file on the board gone, on the install
//! and on any rollback across the line.
//!
//! The decision reads the new core's **own bytes** for the tree-store mark
//! ([`lpc_model::manifest::CORE_FS_TREE_MARK`], which a core that mounts the
//! store links as `tree_fs::LP_CORE_FS_TREE_MARK`, a core root of the split
//! link), streaming the image off flash in small reads (a 1.4 MB core does
//! not fit in RAM). It does not read the manifest blob: that is linked into
//! the engine, which an update writes only after this core has booted on
//! trial — reading it here refused every tree-store core, its own kind
//! included
//! (docs/defects/2026-10-16-the-fs-tree-update-guard-reads-a-manifest-the-core-does-not-carry.md).
//! A core without the mark is refused: every littlefs core, and every
//! tree-store dev core built before the mark existed.
//!
//! The other half — a littlefs image that holds on tree-store magic instead
//! of formatting — is `lp_fs::holds_tree_store`; while it holds one, that
//! build's update session runs this guard too (ADR amended 2026-10-16).

use lpc_model::manifest::CORE_FS_TREE_MARK;

/// What a core image says about the tree store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreStoreVerdict {
    /// It carries the tree-store mark: it mounts the store.
    KeepsTheStore,
    /// It does not: it would format the store (a littlefs core).
    WouldFormat,
    /// A read of it failed: nothing says it keeps the store.
    Unreadable,
}

impl CoreStoreVerdict {
    /// The board's words for a refusal (`None` when the install may go on).
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Self::KeepsTheStore => None,
            Self::WouldFormat => Some(
                "this board holds a tree-store filesystem; that core would format it \
                 (it does not mount the tree store)",
            ),
            Self::Unreadable => Some(
                "this board holds a tree-store filesystem; that core could not be read back, \
                 so nothing says it keeps it",
            ),
        }
    }
}

/// Bytes read per step.
const STEP: usize = 512;

/// The mark, stored masked: an image that links this guard must not carry a
/// contiguous copy of the mark outside a real one — a littlefs core that
/// holds a tree store links the guard, and would read as a tree core if it
/// did.
const MASK: u8 = 0xA5;
const MARK_LEN: usize = CORE_FS_TREE_MARK.len();
const MARK_MASKED: [u8; MARK_LEN] = masked(CORE_FS_TREE_MARK);

const fn masked<const N: usize>(marker: &str) -> [u8; N] {
    let bytes = marker.as_bytes();
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = bytes[i] ^ MASK;
        i += 1;
    }
    out
}

/// The mark unmasked at run time; `black_box` keeps the compiler from
/// folding it back into a contiguous constant.
fn unmasked<const N: usize>(masked: &[u8; N]) -> [u8; N] {
    let key = core::hint::black_box(MASK);
    let mut out = *masked;
    for b in &mut out {
        *b ^= key;
    }
    out
}

/// Scan the `len`-byte core image `read` serves (`read(offset, buf)` fills
/// `buf` from the image at `offset`; `false` = a read failed) for the
/// tree-store mark, and decide.
pub fn core_store_verdict(
    len: u32,
    mut read: impl FnMut(u32, &mut [u8]) -> bool,
) -> CoreStoreVerdict {
    match find_in_image(len, &unmasked(&MARK_MASKED), &mut read) {
        Ok(Some(_)) => CoreStoreVerdict::KeepsTheStore,
        Ok(None) => CoreStoreVerdict::WouldFormat,
        Err(()) => CoreStoreVerdict::Unreadable,
    }
}

/// The offset of `needle`'s first occurrence in the image, reading `STEP`
/// bytes at a time with an overlap of `needle.len() - 1`; `Err` when a read
/// failed.
fn find_in_image(
    len: u32,
    needle: &[u8],
    read: &mut impl FnMut(u32, &mut [u8]) -> bool,
) -> Result<Option<u32>, ()> {
    let mut buf = alloc::vec![0u8; STEP];
    let keep = needle.len() - 1;
    let mut at = 0u32;
    while at < len {
        let n = ((len - at) as usize).min(STEP);
        if !read(at, &mut buf[..n]) {
            return Err(());
        }
        if let Some(i) = find(&buf[..n], needle) {
            return Ok(Some(at + i as u32));
        }
        if at + n as u32 >= len {
            break;
        }
        at += (n - keep) as u32;
    }
    Ok(None)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    use super::*;
    use lpc_model::manifest::{MANIFEST_BLOB_BEGIN, MANIFEST_BLOB_END};

    #[test]
    fn a_core_with_the_mark_keeps_the_store() {
        let image = core(Some(CORE_FS_TREE_MARK), 3_000);
        assert_eq!(verdict(&image), CoreStoreVerdict::KeepsTheStore);
        assert_eq!(CoreStoreVerdict::KeepsTheStore.refusal(), None);
    }

    #[test]
    fn a_littlefs_core_would_format_it_and_is_refused() {
        let image = core(None, 3_000);
        assert_eq!(verdict(&image), CoreStoreVerdict::WouldFormat);
        assert!(
            CoreStoreVerdict::WouldFormat
                .refusal()
                .unwrap()
                .contains("would format it")
        );
    }

    /// The defect this guard was rewritten for: a core whose manifest says
    /// `fs.tree` but whose bytes carry no mark (the manifest is the
    /// engine's) is not taken on the manifest's word — and a core with the
    /// mark needs no manifest.
    #[test]
    fn the_manifest_is_not_what_decides() {
        let mut manifest_only = std::vec![0x13u8; 1_000];
        manifest_only.extend_from_slice(MANIFEST_BLOB_BEGIN.as_bytes());
        manifest_only.extend_from_slice(br#"{"features":["fs.tree"]}"#);
        manifest_only.extend_from_slice(MANIFEST_BLOB_END.as_bytes());
        manifest_only.extend(core::iter::repeat_n(0x6fu8, 2_000));
        assert_eq!(verdict(&manifest_only), CoreStoreVerdict::WouldFormat);
        assert_eq!(
            verdict(&core(Some(CORE_FS_TREE_MARK), 100)),
            CoreStoreVerdict::KeepsTheStore
        );
    }

    #[test]
    fn a_mark_that_is_cut_short_or_masked_is_not_it() {
        let mark = CORE_FS_TREE_MARK.as_bytes();
        let mut image = std::vec![0x13u8; 600];
        image.extend_from_slice(&mark[..mark.len() - 1]);
        image.extend(core::iter::repeat_n(0x6fu8, 600));
        assert_eq!(verdict(&image), CoreStoreVerdict::WouldFormat);
        // The guard's own masked copy is not a mark either.
        let mut image = std::vec![0x13u8; 600];
        image.extend_from_slice(&MARK_MASKED);
        image.extend(core::iter::repeat_n(0x6fu8, 600));
        assert_eq!(verdict(&image), CoreStoreVerdict::WouldFormat);
    }

    #[test]
    fn a_mark_across_a_read_boundary_is_found() {
        // Every offset of the mark around the 512-byte steps.
        for pad in 480..560 {
            let image = core(Some(CORE_FS_TREE_MARK), pad);
            assert_eq!(
                verdict(&image),
                CoreStoreVerdict::KeepsTheStore,
                "pad {pad}"
            );
        }
    }

    #[test]
    fn a_failed_read_is_refused() {
        let image = core(Some(CORE_FS_TREE_MARK), 100);
        let v = core_store_verdict(image.len() as u32, |_, _| false);
        assert_eq!(v, CoreStoreVerdict::Unreadable);
        assert!(CoreStoreVerdict::Unreadable.refusal().is_some());
        let blank = alloc::vec![0xFFu8; 10_000];
        assert_eq!(verdict(&blank), CoreStoreVerdict::WouldFormat);
    }

    // ---- helpers -----------------------------------------------------------

    extern crate alloc;

    /// A core image: `pad` bytes of code, the mark (or none), more code.
    fn core(mark: Option<&str>, pad: usize) -> Vec<u8> {
        let mut out = alloc::vec![0x13u8; pad];
        if let Some(mark) = mark {
            out.extend_from_slice(mark.as_bytes());
        }
        out.extend(core::iter::repeat_n(0x6fu8, 2_000));
        out
    }

    fn verdict(image: &[u8]) -> CoreStoreVerdict {
        core_store_verdict(image.len() as u32, |at, buf| {
            let at = at as usize;
            match image.get(at..at + buf.len()) {
                Some(src) => {
                    buf.copy_from_slice(src);
                    true
                }
                None => false,
            }
        })
    }
}
