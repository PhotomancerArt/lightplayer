//! Growing the partition a store lives in (FORMAT.md "Writing and
//! killing", growing the partition). The store records no identity, and
//! `N` is not on flash: mounted over a bigger partition, it reads the added
//! sectors like its own. A sector there that still holds a trusted header —
//! flash an earlier layout used, another store whose seqs restarted at its
//! own format — would be read as this store's, and a root of its with a
//! higher seq and a complete closure adopted. So whoever enlarges the
//! partition (the USB layout migration, a loader, a core) first kills every
//! header in the added range that does not read erased: the lazy format's
//! kill, applied to the range.

use crate::flash::Flash;
use crate::sector_header::{KILLED_SECTOR_HEADER, SECTOR_HEADER_LEN};

/// Kill the header of every sector from `first` to the end of `flash` (the
/// enlarged partition) that does not read all `0xFF`, and read it back; a
/// kill that does not read back is finished by an erase (the range is not
/// the store's yet, so erasing it loses nothing). Idempotent, and safe to
/// run again after a power cut. Returns how many headers it killed that did
/// not already read killed.
pub fn kill_added_sectors<F: Flash>(flash: &mut F, first: u32) -> Result<u32, F::Error> {
    let size = flash.sector_size();
    let mut killed = 0;
    for s in first..flash.sector_count() {
        let mut h = [0u8; SECTOR_HEADER_LEN as usize];
        flash.read(s * size, &mut h)?;
        if h.iter().all(|&b| b == 0xFF) {
            continue;
        }
        // Killed again even when it reads killed: a kill a power cut left
        // weak reads killed on some reads only (programming zeros over zeros
        // finishes it and changes nothing else).
        let was_killed = h == KILLED_SECTOR_HEADER;
        flash.program(s * size, &KILLED_SECTOR_HEADER)?;
        flash.read(s * size, &mut h)?;
        if h != KILLED_SECTOR_HEADER {
            flash.erase_sector(s)?;
        }
        killed += u32::from(!was_killed);
    }
    Ok(killed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{formatted, mount, snapshot, text};
    use crate::{SoftSha256, StoreConfig, TreeStore};
    use lp_nor_sim::{NorFlashSim, NorGeometry};

    /// Another store's sectors in the range a partition grows into: its
    /// roots have higher seqs and complete closures, so mounted over the
    /// bigger partition untouched, they win and this store's files are
    /// gone. Killed first, the grown store mounts as itself.
    #[test]
    fn a_grown_partition_reads_only_its_own_store() {
        let c = StoreConfig::default();
        // The store that stays: 8 sectors, a few commits.
        let mut ours = mount(formatted(NorGeometry::c6(8), &c), &c);
        ours.put("/ours.json", &text(1, 300)).unwrap();
        let want = snapshot(&mut ours);
        let ours = ours.into_flash();
        // Another store on 8 sectors, with many more commits (higher root
        // seqs), whose flash the partition will grow over.
        let mut other = mount(formatted(NorGeometry::c6(8), &c), &c);
        for i in 0..40u64 {
            other.put("/theirs.json", &text(100 + i, 200)).unwrap();
        }
        let other = other.into_flash();
        let grown = || {
            let mut f = NorFlashSim::new(NorGeometry::c6(16));
            let mut cells = alloc::vec![0u8; 8 * 4096];
            ours.peek(0, &mut cells);
            f.program(0, &cells).unwrap();
            other.peek(0, &mut cells);
            f.program(8 * 4096, &cells).unwrap();
            f
        };
        // Untouched, the other store's newer root wins: the hazard.
        let mut st = mount(grown(), &c);
        assert_ne!(snapshot(&mut st), want, "the hazard did not reproduce");
        // Killed first, the grown store is ours, and the added sectors are
        // free to use.
        let mut f = grown();
        assert!(kill_added_sectors(&mut f, 8).unwrap() > 0);
        assert_eq!(kill_added_sectors(&mut f, 8).unwrap(), 0, "idempotent");
        let mut st = mount(f, &c);
        assert_eq!(snapshot(&mut st), want);
        for i in 0..30u64 {
            st.put("/more.json", &text(200 + i, 3000)).unwrap();
        }
        let mut st = match TreeStore::mount(st.into_flash(), SoftSha256, c.clone()) {
            Ok(s) => s,
            Err(_) => panic!("remount"),
        };
        assert_eq!(st.get("/ours.json").unwrap().unwrap(), text(1, 300));
    }
}
