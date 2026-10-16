//! T1: the content-addressed copy-on-write tree store (`lp-tree-store` v1).
//! Dials: `record_max`, `gc_policy` (`greedy` | `cost_benefit`), `reserve`,
//! `txn_delta_max`, `codec` (`stored` | `host_deflate`).
//!
//! A workload step is one store transaction (`begin` at the step's first
//! write, `commit` at its end), so T1 is scored step-atomic. With
//! `codec=host_deflate` every write but the board's own hot files (`…/.lp/
//! panel.json`, written by the device) arrives as the wire will carry it
//! after M6: host-deflated chunks of ≤ 4 KiB logical, `put_chunk_deflated`
//! at offset 0 then the running size. `stored` writes every file stored.

use lp_nor_sim::{NorError, NorFlashSim};
use lp_tree_store::{GcPolicy, SoftSha256, StoreConfig, TreeStore, host_deflate_chunks, is_hot};

use crate::{Candidate, CandidateConfig, CandidateReport, CandidateStore, StoreError};

pub struct TreeStoreCandidate;

/// How the adapter writes non-hot files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T1Codec {
    Stored,
    HostDeflate,
}

/// The store's config and the codec dial (defaults = the crate's, stored).
pub fn tree_store_config(cfg: &CandidateConfig) -> Result<(StoreConfig, T1Codec), StoreError> {
    let mut c = StoreConfig::default();
    let mut codec = T1Codec::Stored;
    for (k, v) in &cfg.dials {
        let num = || {
            v.parse::<u32>()
                .map_err(|_| StoreError::Other(format!("dial {k}={v}: not a number")))
        };
        match k.as_str() {
            "record_max" => c.record_max = num()?,
            "reserve" => c.reserve = num()?,
            "txn_delta_max" => c.txn_delta_max = num()?,
            "gc_policy" => {
                c.gc_policy = match v.as_str() {
                    "greedy" => GcPolicy::Greedy,
                    "cost_benefit" => GcPolicy::CostBenefit,
                    _ => return Err(StoreError::Other(format!("dial gc_policy={v}"))),
                }
            }
            "codec" => {
                codec = match v.as_str() {
                    "stored" => T1Codec::Stored,
                    "host_deflate" => T1Codec::HostDeflate,
                    _ => return Err(StoreError::Other(format!("dial codec={v}"))),
                }
            }
            _ => return Err(StoreError::Other(format!("unknown t1 dial {k}"))),
        }
    }
    Ok((c, codec))
}

fn map_err(e: lp_tree_store::StoreError<NorError>) -> StoreError {
    use lp_tree_store::StoreError as T;
    match e {
        T::NoSpace => StoreError::NoSpace,
        T::Flash(NorError::PowerLost) => StoreError::PowerLost,
        T::Flash(e) => StoreError::Other(format!("flash: {e:?}")),
        T::Corrupt(s) => StoreError::Corrupt(s.into()),
        e => StoreError::Other(format!("{e:?}")),
    }
}

impl Candidate for TreeStoreCandidate {
    fn name(&self) -> &str {
        "t1"
    }

    fn format(&self, flash: &mut NorFlashSim, cfg: &CandidateConfig) -> Result<(), StoreError> {
        TreeStore::format(flash, SoftSha256, tree_store_config(cfg)?.0)
            .map(drop)
            .map_err(|(e, _, _)| map_err(e))
    }

    fn mount(
        &self,
        flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Box<dyn CandidateStore>, (StoreError, NorFlashSim)> {
        let (sc, codec) = match tree_store_config(cfg) {
            Ok(c) => c,
            Err(e) => return Err((e, flash)),
        };
        match TreeStore::mount(flash, SoftSha256, sc) {
            Ok(store) => Ok(Box::new(TreeStoreAdapter { store, codec })),
            Err((e, flash, _)) => Err((map_err(e), flash)),
        }
    }
}

struct TreeStoreAdapter {
    store: TreeStore<NorFlashSim, SoftSha256>,
    codec: T1Codec,
}

impl TreeStoreAdapter {
    fn in_step(&mut self) -> Result<(), StoreError> {
        if !self.store.in_transaction() {
            self.store.begin().map_err(map_err)?;
        }
        Ok(())
    }
}

impl CandidateStore for TreeStoreAdapter {
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.in_step()?;
        if self.codec == T1Codec::Stored || is_hot(path) {
            return self.store.put(path, bytes).map_err(map_err);
        }
        let rm = self.store.config().record_max;
        let mut offset = 0;
        for c in host_deflate_chunks(&mut SoftSha256, bytes, rm) {
            self.store
                .put_chunk_deflated(path, offset, c.logical_len, Some(c.id), &c.deflated)
                .map_err(map_err)?;
            offset += c.logical_len;
        }
        Ok(())
    }

    fn get(&mut self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.store.get(path).map_err(map_err)
    }

    fn delete_prefix(&mut self, prefix: &str) -> Result<(), StoreError> {
        self.in_step()?;
        self.store.delete_prefix(prefix).map_err(map_err)
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.store.list(prefix).map_err(map_err)
    }

    fn commit(&mut self) -> Result<(), StoreError> {
        let r = self.store.commit().map_err(map_err);
        if r.is_err() {
            // A step that did not commit leaves nothing: a store with the
            // transaction still open would carry it into the next step.
            let _ = self.store.abort();
        }
        r
    }

    fn into_flash(self: Box<Self>) -> NorFlashSim {
        self.store.into_flash()
    }

    fn flash_snapshot(&self) -> NorFlashSim {
        self.store.flash().clone()
    }

    fn report(&self) -> CandidateReport {
        let st = self.store.stats();
        let sectors = self.store.flash().geometry().sector_count;
        let mut extra = std::collections::BTreeMap::new();
        for (k, v) in [
            ("index_entries", st.index_entries as f64),
            ("index_ram_bytes", st.index_ram_bytes as f64),
            ("sector_table_ram_bytes", st.sector_table_ram_bytes as f64),
            ("resident_ram_bytes", st.resident_ram_bytes as f64),
            ("transient_peak_bytes", st.transient_peak_bytes as f64),
            ("dedup_hits", st.dedup_hits as f64),
            ("marks", st.marks as f64),
            ("gc_copies", st.gc_copies as f64),
            ("gc_copy_bytes", st.gc_copy_bytes as f64),
            ("gc_runs", st.gc_runs as f64),
            ("records_written", st.records_written as f64),
            ("record_bytes_written", st.record_bytes_written as f64),
            ("verify_failures", st.verify_failures as f64),
            ("retired_sectors", st.retired_sectors as f64),
            ("free_sectors", self.store.free_sectors() as f64),
        ] {
            extra.insert(k.to_string(), v);
        }
        CandidateReport {
            ram_bytes: st.resident_ram_bytes as u64,
            used_sectors: Some(sectors - self.store.free_sectors()),
            step_atomic: true,
            extra,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver_exhaustive::{SweepParams, sweep_exhaustive};
    use crate::{CorpusSet, Scoreboard, WorkloadKind, WorkloadSpec};

    #[test]
    fn dials_parse() {
        let cfg = CandidateConfig::new(32)
            .with_dial("codec", "host_deflate")
            .with_dial("record_max", "512")
            .with_dial("gc_policy", "greedy");
        let (c, codec) = tree_store_config(&cfg).unwrap();
        assert_eq!(codec, T1Codec::HostDeflate);
        assert_eq!(c.record_max, 512);
        assert!(tree_store_config(&CandidateConfig::new(32).with_dial("nope", "1")).is_err());
        assert!(
            tree_store_config(&CandidateConfig::new(32).with_dial("codec", "deflate_dict"))
                .is_err()
        );
    }

    #[test]
    fn a_small_sweep_has_no_failures() {
        for codec in ["stored", "host_deflate"] {
            let wl = CorpusSet::new(None)
                .build(&WorkloadSpec::new(WorkloadKind::Save, "syn:3:300", 1))
                .unwrap();
            let params = SweepParams {
                seeds: vec![1],
                max_cuts_per_step: Some(24),
                steps: Some(vec![2, 3]),
                ..Default::default()
            };
            let out = sweep_exhaustive(
                &TreeStoreCandidate,
                &CandidateConfig::new(32).with_dial("codec", codec),
                &wl,
                &params,
                &Scoreboard::memory(),
            );
            assert!(
                out.iter()
                    .all(|s| s.cases > 0 && s.failures == 0 && s.non_atomic == 0),
                "{codec}: {out:?}"
            );
        }
    }
}
