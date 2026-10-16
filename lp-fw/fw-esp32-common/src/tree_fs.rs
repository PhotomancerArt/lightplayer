//! The boot mount of an `fs-tree` board's filesystem: the tree store's
//! verdict, and what the board does with it (plan
//! `2026-10-08-2339-tree-store-firmware-and-emulator`, D1 and D2). The twin
//! of `lp_fs::LpFsFlash::init_guarded` for the littlefs build, generic over
//! the chip's flash and hasher (both injected: this crate holds no chip
//! fact).
//!
//! | mount says | the board |
//! |---|---|
//! | mounted | serves it ([`TreeFsInit::Mounted`]) |
//! | `NoStore` (blank, littlefs, foreign, an interrupted first format) | asks `may_format` (on the C6: is a pre-repartition filesystem waiting?) — `Format`: formats and serves it ([`TreeFsInit::Formatted`]); `Hold`: writes nothing ([`TreeFsInit::Held`]) |
//! | anything else (`Damaged`, `Unsupported`, a flash that would not read) | **refuses**: writes nothing, keeps the files for `lp-cli hardware tree extract` ([`TreeFsInit::Refused`]) |
//!
//! It never formats a store it refused. The caller serves a RAM filesystem
//! for `Held` and `Refused` and locks access for both
//! (`FsBootState::LegacyHeld` / `FsBootState::Refused`): a RAM device
//! store would read as missing, which is open by default, on a board whose
//! own access list waits on its flash.

use core::fmt::Debug;

use lp_tree_store::{
    Flash, LpFsTree, MountSummary, ObjectHasher, StoreConfig, StoreError, TreeStore,
};

pub use crate::lp_fs::FormatVerdict;

/// How [`init_tree_guarded`] came out.
pub enum TreeFsInit<F: Flash, H: ObjectHasher> {
    /// The store on flash mounted.
    Mounted(LpFsTree<F, H>, MountSummary),
    /// No store was there; the partition was formatted and the empty store
    /// mounted.
    Formatted(LpFsTree<F, H>, MountSummary),
    /// No store was there, and the verdict said [`FormatVerdict::Hold`]:
    /// nothing was written. The flash comes back.
    Held(F),
    /// A store's records the mount would not adopt — a newer or damaged
    /// store header, a store with no complete root, a flash that would not
    /// read. Nothing was written. `why` is the store's word for it; the
    /// flash comes back.
    Refused { why: &'static str, flash: F },
    /// No store was there, and the format failed (the flash failed). The
    /// caller serves a RAM filesystem.
    FormatFailed,
}

/// Mount the store on `flash`; format only a flash with no store on it, and
/// only when `may_format` says so. The caller prints the verdict's boot
/// word (it owns the console); this logs only the start of a format, the
/// one step that takes seconds on silicon (D4), and a failed one.
pub fn init_tree_guarded<F, H>(
    flash: F,
    hasher: H,
    cfg: StoreConfig,
    may_format: impl FnOnce(&mut F) -> FormatVerdict,
) -> TreeFsInit<F, H>
where
    F: Flash + 'static,
    F::Error: Debug,
    H: ObjectHasher + 'static,
{
    let (mut flash, hasher) = match TreeStore::mount(flash, hasher, cfg.clone()) {
        Ok(st) => {
            let s = st.summary();
            return TreeFsInit::Mounted(LpFsTree::new(st), s);
        }
        Err((StoreError::NoStore, flash, hasher)) => (flash, hasher),
        Err((e, flash, _)) => {
            return TreeFsInit::Refused {
                why: refusal(&e),
                flash,
            };
        }
    };
    if may_format(&mut flash) == FormatVerdict::Hold {
        return TreeFsInit::Held(flash);
    }
    log::warn!("[FS] tree store: no store here, formatting partition...");
    match TreeStore::format(flash, hasher, cfg) {
        Ok(st) => {
            let s = st.summary();
            TreeFsInit::Formatted(LpFsTree::new(st), s)
        }
        Err((e, _, _)) => {
            log::warn!("[FS] tree store format failed: {e:?}");
            TreeFsInit::FormatFailed
        }
    }
}

/// The store's word for a mount it would not adopt (never `NoStore`).
fn refusal<E>(e: &StoreError<E>) -> &'static str {
    match e {
        StoreError::Unsupported(why) | StoreError::Damaged(why) | StoreError::Corrupt(why) => why,
        StoreError::BadConfig(why) => why,
        StoreError::Flash(_) => "the flash did not read",
        _ => "mount failed",
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use lp_nor_sim::{NorFlashSim, NorGeometry};
    use lp_tree_store::SoftSha256;
    use lpfs::{LpFs, LpPath};

    use super::*;

    #[test]
    fn blank_flash_formats_and_serves() {
        let init = init_tree_guarded(blank(), SoftSha256, cfg(), |_| FormatVerdict::Format);
        let TreeFsInit::Formatted(fs, s) = init else {
            panic!("not formatted");
        };
        assert_eq!((s.sectors, s.root_seq), (SECTORS, 1));
        fs.write_file(LpPath::new("/a.json"), b"{}").unwrap();
    }

    #[test]
    fn a_store_mounts_with_its_files() {
        let f = with_a_file();
        let TreeFsInit::Mounted(fs, s) = init_tree_guarded(f, SoftSha256, cfg(), |_| never())
        else {
            panic!("not mounted");
        };
        assert_eq!(fs.read_file(LpPath::new("/a.json")).unwrap(), b"{\"a\":1}");
        assert!(s.root_seq >= 2);
    }

    #[test]
    fn hold_writes_nothing() {
        let f = blank();
        let init = init_tree_guarded(f, SoftSha256, cfg(), |_| FormatVerdict::Hold);
        let TreeFsInit::Held(f) = init else {
            panic!("not held");
        };
        assert_eq!(image(&f), image(&blank()));
    }

    #[test]
    fn a_damaged_store_is_refused_and_never_formatted() {
        // The roots' sector erased: the files' records stay, no root names
        // them.
        let mut f = with_a_file();
        let before = image(&f);
        for s in 0..SECTORS {
            let mut h = [0u8; 24];
            f.peek(s * 4096, &mut h);
            // A hot sector's header names head kind 1 (FORMAT.md "Sector").
            if h[..4] == 0x3153_544Cu32.to_le_bytes() && h[6] == 1 {
                f.erase_sector(s).unwrap();
            }
        }
        let damaged = image(&f);
        assert_ne!(damaged, before);
        let init = init_tree_guarded(f, SoftSha256, cfg(), |_| never());
        let TreeFsInit::Refused { why, flash } = init else {
            panic!("not refused");
        };
        assert_eq!(why, "no complete root");
        assert_eq!(image(&flash), damaged, "the refusal wrote to flash");
    }

    #[test]
    fn a_newer_header_is_refused_and_never_formatted() {
        let mut f = with_a_file();
        // The first trusted sector's version, made 4 and its CRC left: a
        // magic in front of a newer version is refused before its CRC.
        let s = (0..SECTORS)
            .find(|&s| {
                let mut h = [0u8; 4];
                f.peek(s * 4096, &mut h);
                h == 0x3153_544Cu32.to_le_bytes()
            })
            .unwrap();
        let mut cells = std::vec![0u8; 4096];
        f.peek(s * 4096, &mut cells);
        cells[4..6].copy_from_slice(&4u16.to_le_bytes());
        f.erase_sector(s).unwrap();
        f.program(s * 4096, &cells).unwrap();
        let newer = image(&f);
        let init = init_tree_guarded(f, SoftSha256, cfg(), |_| never());
        let TreeFsInit::Refused { why, flash } = init else {
            panic!("not refused");
        };
        assert_eq!(why, "newer format");
        assert_eq!(image(&flash), newer);
    }

    // ---- helpers -----------------------------------------------------------

    const SECTORS: u32 = 16;

    fn cfg() -> StoreConfig {
        StoreConfig::default()
    }

    fn blank() -> NorFlashSim {
        NorFlashSim::new(NorGeometry::c6(SECTORS))
    }

    fn never() -> FormatVerdict {
        panic!("asked to format a flash that has a store")
    }

    fn with_a_file() -> NorFlashSim {
        let mut st = match TreeStore::format(blank(), SoftSha256, cfg()) {
            Ok(st) => st,
            Err((e, ..)) => panic!("format: {e:?}"),
        };
        st.put("/a.json", b"{\"a\":1}").unwrap();
        st.put("/projects/p/.lp/panel.json", b"{}").unwrap();
        st.into_flash()
    }

    fn image(f: &NorFlashSim) -> std::vec::Vec<u8> {
        let mut out = std::vec![0u8; (SECTORS * 4096) as usize];
        f.peek(0, &mut out);
        out
    }
}
