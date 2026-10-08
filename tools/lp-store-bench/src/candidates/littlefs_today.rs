//! **F1 — littlefs as used today.** One littlefs file per document, folders
//! as the paths say, made on demand; `delete_prefix` is the firmware's
//! recursive delete; `commit` is a no-op (littlefs is atomic per file
//! operation, not per step), so `step_atomic` is false.
//!
//! Like the firmware, a document is written in place (`open` with
//! `CREATE | TRUNC`, write, close). littlefs commits a *new* file's directory
//! entry at `open`, before its data, so a cut between the two leaves the new
//! path present and empty: that is littlefs's documented behaviour, kept here
//! on purpose because it is what the product does. (One difference from the
//! firmware: its `Filesystem::write_file` drops the file and so ignores the
//! close's error; this adapter checks it, so a full disk says `NoSpace`.)

use lp_nor_sim::NorFlashSim;

use crate::candidates::littlefs_volume::{LfsVolume, lfs_ram_bytes};
use crate::{Candidate, CandidateConfig, CandidateReport, CandidateStore, StoreError};

pub struct LittlefsToday;

impl Candidate for LittlefsToday {
    fn name(&self) -> &str {
        "f1"
    }

    fn format(&self, flash: &mut NorFlashSim, cfg: &CandidateConfig) -> Result<(), StoreError> {
        LfsVolume::format(flash, cfg)
    }

    fn mount(
        &self,
        flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Box<dyn CandidateStore>, (StoreError, NorFlashSim)> {
        Ok(Box::new(LittlefsTodayStore {
            vol: LfsVolume::mount(flash, cfg)?,
        }))
    }
}

struct LittlefsTodayStore {
    vol: LfsVolume,
}

impl CandidateStore for LittlefsTodayStore {
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.vol.write_file(path, bytes)
    }

    fn get(&mut self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.vol.read_file(path)
    }

    fn delete_prefix(&mut self, prefix: &str) -> Result<(), StoreError> {
        self.vol.delete_prefix(prefix, &|_| false)
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.vol.files_under(prefix)
    }

    fn commit(&mut self) -> Result<(), StoreError> {
        Ok(())
    }

    fn into_flash(self: Box<Self>) -> NorFlashSim {
        self.vol.into_flash()
    }

    fn flash_snapshot(&self) -> NorFlashSim {
        self.vol.snapshot()
    }

    fn report(&self) -> CandidateReport {
        let mut extra = std::collections::BTreeMap::new();
        extra.insert("open_files_max".into(), 1.0);
        CandidateReport {
            // One file open at a time (a put, a get).
            ram_bytes: lfs_ram_bytes(1),
            used_sectors: self.vol.used_blocks(),
            step_atomic: false,
            extra,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::candidate_tests::{round_trip, small_sweep};

    #[test]
    fn round_trips_fault_free() {
        round_trip(&LittlefsToday);
    }

    #[test]
    fn small_exhaustive_sweep_runs() {
        // littlefs creates a new file's entry before its data, so a cut can
        // leave a new path empty: failures are a result here, not asserted.
        let s = small_sweep(&LittlefsToday);
        eprintln!("f1 small sweep: {s:?}");
        assert!(s.cases > 0);
    }
}
