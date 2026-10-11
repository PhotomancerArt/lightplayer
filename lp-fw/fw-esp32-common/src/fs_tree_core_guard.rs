//! The `fs-tree` board's update guard (plan
//! `2026-10-08-2339-tree-store-firmware-and-emulator`, D8): a core install
//! whose build lacks `fs-tree` is refused, because that core would find no
//! littlefs on `lpfs`, take the tree store for an unformatted partition, and
//! format it — every file on the board gone, on the install and on any
//! rollback across the line.
//!
//! The decision reads the new core's own embedded manifest core (the
//! `lp_embed_manifest_core!` blob, between `MANIFEST_BLOB_BEGIN` and
//! `MANIFEST_BLOB_END`) for the `fs.tree` feature, streaming the image off
//! flash in small reads (a 1.4 MB core does not fit in RAM). A core with no
//! readable manifest is refused too: nothing says it keeps the store.
//!
//! The other half — a littlefs image that holds on tree-store magic instead
//! of formatting — is product-image code and the adoption round's.

use lpc_model::LpFeature;
use lpc_model::manifest::{MANIFEST_BLOB_BEGIN, MANIFEST_BLOB_END};

/// What a core image says about the tree store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreStoreVerdict {
    /// Its manifest lists `fs.tree`: it mounts the store.
    KeepsTheStore,
    /// Its manifest does not: it would format the store.
    WouldFormat,
    /// No manifest blob could be read from it.
    NoManifest,
}

impl CoreStoreVerdict {
    /// The board's words for a refusal (`None` when the install may go on).
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Self::KeepsTheStore => None,
            Self::WouldFormat => Some(
                "this board holds a tree-store filesystem; that core would format it \
                 (its build lacks fs-tree)",
            ),
            Self::NoManifest => Some(
                "this board holds a tree-store filesystem; that core names no features, so \
                 nothing says it keeps it",
            ),
        }
    }
}

/// Bytes read per step.
const STEP: usize = 512;
/// The most manifest JSON read after its begin marker.
const MANIFEST_MAX: usize = 4096;

/// Scan the `len`-byte core image `read` serves (`read(offset, buf)` fills
/// `buf` from the image at `offset`; `false` = a read failed) for its
/// manifest core, and decide.
pub fn core_store_verdict(
    len: u32,
    mut read: impl FnMut(u32, &mut [u8]) -> bool,
) -> CoreStoreVerdict {
    let begin = MANIFEST_BLOB_BEGIN.as_bytes();
    let Some(json_at) = find_in_image(len, begin, &mut read).map(|at| at + begin.len() as u32)
    else {
        return CoreStoreVerdict::NoManifest;
    };
    let room = (len - json_at).min(MANIFEST_MAX as u32) as usize;
    let mut json = alloc::vec![0u8; room];
    if !read(json_at, &mut json) {
        return CoreStoreVerdict::NoManifest;
    }

    let Some(end) = find(&json, MANIFEST_BLOB_END.as_bytes()) else {
        return CoreStoreVerdict::NoManifest;
    };
    // The `features` array only: `"features"`, its `[`, up to its `]` (no
    // feature name holds a bracket).
    let json = &json[..end];
    let Some(key) = find(json, b"\"features\"") else {
        return CoreStoreVerdict::NoManifest;
    };
    let after = &json[key..];
    let (Some(open), Some(close)) = (find(after, b"["), find(after, b"]")) else {
        return CoreStoreVerdict::NoManifest;
    };
    if close < open {
        return CoreStoreVerdict::NoManifest;
    }
    let features = &after[open..close];
    let mut quoted = [0u8; 32];
    let name = LpFeature::FsTree.wire_name().as_bytes();
    quoted[0] = b'"';
    quoted[1..=name.len()].copy_from_slice(name);
    quoted[name.len() + 1] = b'"';
    if find(features, &quoted[..name.len() + 2]).is_some() {
        CoreStoreVerdict::KeepsTheStore
    } else {
        CoreStoreVerdict::WouldFormat
    }
}

/// The offset of `needle`'s first occurrence in the image, reading `STEP`
/// bytes at a time with an overlap of `needle.len() - 1`.
fn find_in_image(
    len: u32,
    needle: &[u8],
    read: &mut impl FnMut(u32, &mut [u8]) -> bool,
) -> Option<u32> {
    let mut buf = alloc::vec![0u8; STEP];
    let keep = needle.len() - 1;
    let mut at = 0u32;
    while at < len {
        let n = ((len - at) as usize).min(STEP);
        if !read(at, &mut buf[..n]) {
            return None;
        }
        if let Some(i) = find(&buf[..n], needle) {
            return Some(at + i as u32);
        }
        if at + n as u32 >= len {
            break;
        }
        at += (n - keep) as u32;
    }
    None
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

    #[test]
    fn a_core_with_fs_tree_keeps_the_store() {
        let image = core(
            r#"{"features":["node.shader","fs.tree"],"wireProto":42}"#,
            3_000,
        );
        assert_eq!(verdict(&image), CoreStoreVerdict::KeepsTheStore);
        assert_eq!(CoreStoreVerdict::KeepsTheStore.refusal(), None);
    }

    #[test]
    fn a_littlefs_core_would_format_it_and_is_refused() {
        let image = core(
            r#"{"features":["node.shader","gfx.lpvm"],"wireProto":42}"#,
            3_000,
        );
        assert_eq!(verdict(&image), CoreStoreVerdict::WouldFormat);
        assert!(
            CoreStoreVerdict::WouldFormat
                .refusal()
                .unwrap()
                .contains("would format it")
        );
    }

    #[test]
    fn a_name_that_only_contains_fs_tree_is_not_it() {
        let image = core(r#"{"features":["fs.tree-v2"],"note":"fs.tree"}"#, 100);
        assert_eq!(verdict(&image), CoreStoreVerdict::WouldFormat);
    }

    #[test]
    fn a_marker_across_a_read_boundary_is_found() {
        // Every offset of the begin marker around the 512-byte steps.
        for pad in 480..560 {
            let image = core(r#"{"features":["fs.tree"]}"#, pad);
            assert_eq!(
                verdict(&image),
                CoreStoreVerdict::KeepsTheStore,
                "pad {pad}"
            );
        }
    }

    #[test]
    fn no_manifest_or_a_failed_read_is_refused() {
        let blank = alloc::vec![0xFFu8; 10_000];
        assert_eq!(verdict(&blank), CoreStoreVerdict::NoManifest);
        let image = core(r#"{"features":["fs.tree"]}"#, 100);
        let v = core_store_verdict(image.len() as u32, |_, _| false);
        assert_eq!(v, CoreStoreVerdict::NoManifest);
        assert!(CoreStoreVerdict::NoManifest.refusal().is_some());
        // A begin marker with no end within reach.
        let mut cut = image.clone();
        let end = MANIFEST_BLOB_END.as_bytes();
        let at = cut.windows(end.len()).position(|w| w == end).unwrap();
        cut[at] = 0;
        assert_eq!(verdict(&cut), CoreStoreVerdict::NoManifest);
    }

    // ---- helpers -----------------------------------------------------------

    extern crate alloc;

    /// A core image: `pad` bytes of code, the manifest blob, more code.
    fn core(json: &str, pad: usize) -> Vec<u8> {
        let mut out = alloc::vec![0x13u8; pad];
        out.extend_from_slice(MANIFEST_BLOB_BEGIN.as_bytes());
        out.extend_from_slice(json.as_bytes());
        out.extend_from_slice(MANIFEST_BLOB_END.as_bytes());
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
