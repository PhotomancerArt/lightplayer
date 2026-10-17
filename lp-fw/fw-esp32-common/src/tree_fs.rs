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
/// word (it owns the console); this logs only the start of a format (two
/// erases since the format went lazy) and a failed one.
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
            return TreeFsInit::Mounted(served(st), s);
        }
        Err((StoreError::NoStore, flash, hasher)) => (flash, hasher),
        Err((e, flash, _)) => {
            return TreeFsInit::Refused {
                why: refusal(&e),
                flash,
            };
        }
    };
    // Anything but `Format` writes nothing (the C6's tree build answers only
    // `Format` or `Hold`; its refusals are the store's own, above).
    if may_format(&mut flash) != FormatVerdict::Format {
        return TreeFsInit::Held(flash);
    }
    log::warn!("[FS] tree store: no store here, formatting partition...");
    match TreeStore::format(flash, hasher, cfg) {
        Ok(st) => {
            let s = st.summary();
            TreeFsInit::Formatted(served(st), s)
        }
        Err((e, _, _)) => {
            log::warn!("[FS] tree store format failed: {e:?}");
            TreeFsInit::FormatFailed
        }
    }
}

/// The store as the board's filesystem. Its write-only files (access
/// files, the network file: `lpc_access::is_write_only_file_path`) are
/// secret to it: replaced or deleted, their old versions are erased from
/// flash right after the commit, not left as garbage for GC (FORMAT.md
/// "Secret files").
fn served<F: Flash, H: ObjectHasher>(st: TreeStore<F, H>) -> LpFsTree<F, H> {
    LpFsTree::new(st).with_secret_paths(lpc_access::is_write_only_file_path)
}

/// Whether this boot may change the store's **format** on flash: set an
/// incompat flag, write a sector of a newer format version, or start an
/// in-place conversion (FORMAT.md "Versioning", the trial rule). Only on a
/// confirmed boot: a core on an unconfirmed over-the-air trial may yet roll
/// back to the core before it, which must still boot and read the ordinary
/// files — and a store flagged or converted by the trial core would be
/// refused. The same gate as the device access file's
/// ([`lpa_server::access_store::may_migrate_device_store`]).
///
/// Format version 3 defines no flag and no conversion, so nothing calls this
/// yet: it is the gate the first change that needs one must call.
pub fn may_change_store_format(standing: lpa_server::access_store::BootStanding) -> bool {
    lpa_server::access_store::may_migrate_device_store(standing)
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

    /// The served store scrubs the write-only files: the old Wi-Fi
    /// password is not on flash after the network file is replaced.
    #[test]
    fn a_replaced_network_file_leaves_no_old_password_on_flash() {
        let TreeFsInit::Formatted(fs, _) =
            init_tree_guarded(blank(), SoftSha256, cfg(), |_| FormatVerdict::Format)
        else {
            panic!("not formatted");
        };
        let net = LpPath::new("/.lp/network.json");
        fs.write_file(net, b"{\"pass\":\"old-password-1\"}")
            .unwrap();
        fs.write_file(LpPath::new("/a.json"), b"{}").unwrap();
        fs.write_file(net, b"{\"pass\":\"new-password-2\"}")
            .unwrap();
        let flash = fs.with_store(|st| image(st.flash()));
        let has = |s: &[u8]| flash.windows(s.len()).any(|w| w == s);
        assert!(!has(b"old-password-1"));
        assert!(has(b"new-password-2"));
    }

    #[test]
    fn only_a_confirmed_boot_may_change_the_store_format() {
        use lpa_server::access_store::BootStanding;
        assert!(may_change_store_format(BootStanding::Confirmed));
        assert!(!may_change_store_format(BootStanding::UnconfirmedTrial));
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
