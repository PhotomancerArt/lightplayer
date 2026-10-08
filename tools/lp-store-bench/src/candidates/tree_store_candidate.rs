//! T1: the content-addressed copy-on-write tree store (`lp-tree-store`), the
//! candidate the race exists to test. Dials: `record_max`, `gc_policy`
//! (`greedy` | `cost_benefit`), `reserve`, `codec` (`stored` | `deflate` |
//! `deflate_dict`), `dict_size`, `json_tree` (`on` | `off`).

use lp_nor_sim::{NorError, NorFlashSim};
use lp_tree_store::{Codec, GcPolicy, StoreConfig, TreeStore};

use crate::{Candidate, CandidateConfig, CandidateReport, CandidateStore, StoreError};

pub struct TreeStoreCandidate;

/// The store's config from the candidate's dials (defaults = the crate's).
pub fn tree_store_config(cfg: &CandidateConfig) -> Result<StoreConfig, StoreError> {
    let mut c = StoreConfig::default();
    for (k, v) in &cfg.dials {
        let num = || {
            v.parse::<u32>()
                .map_err(|_| StoreError::Other(format!("dial {k}={v}: not a number")))
        };
        match k.as_str() {
            "record_max" => c.record_max = num()?,
            "reserve" => c.reserve = num()?,
            "dict_size" => c.dict_size = num()?,
            "gc_policy" => {
                c.gc_policy = match v.as_str() {
                    "greedy" => GcPolicy::Greedy,
                    "cost_benefit" => GcPolicy::CostBenefit,
                    _ => return Err(StoreError::Other(format!("dial gc_policy={v}"))),
                }
            }
            "codec" => {
                c.codec = match v.as_str() {
                    "stored" => Codec::Stored,
                    "deflate" => Codec::Deflate,
                    "deflate_dict" => Codec::DeflateDict,
                    _ => return Err(StoreError::Other(format!("dial codec={v}"))),
                }
            }
            "json_tree" => c.json_tree = v == "on" || v == "true",
            _ => return Err(StoreError::Other(format!("unknown t1 dial {k}"))),
        }
    }
    Ok(c)
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
        TreeStore::format(flash, &tree_store_config(cfg)?).map_err(map_err)
    }

    fn mount(
        &self,
        flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Box<dyn CandidateStore>, (StoreError, NorFlashSim)> {
        let sc = match tree_store_config(cfg) {
            Ok(c) => c,
            Err(e) => return Err((e, flash)),
        };
        match TreeStore::mount(flash, sc) {
            Ok(store) => Ok(Box::new(TreeStoreAdapter { store })),
            Err((e, flash)) => Err((map_err(e), flash)),
        }
    }
}

struct TreeStoreAdapter {
    store: TreeStore<NorFlashSim>,
}

impl CandidateStore for TreeStoreAdapter {
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.store.put(path, bytes).map_err(map_err)
    }

    fn get(&mut self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.store.get(path).map_err(map_err)
    }

    fn delete_prefix(&mut self, prefix: &str) -> Result<(), StoreError> {
        self.store.delete_prefix(prefix).map_err(map_err)
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.store.list(prefix).map_err(map_err)
    }

    fn commit(&mut self) -> Result<(), StoreError> {
        self.store.commit().map_err(map_err)
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
            ("tree_ram_bytes", st.tree_ram_bytes as f64),
            ("sector_table_ram_bytes", st.sector_table_ram_bytes as f64),
            ("largest_buffer", st.largest_buffer as f64),
            ("dedup_hits", st.dedup_hits as f64),
            ("gc_copies", st.gc_copies as f64),
            ("gc_copy_bytes", st.gc_copy_bytes as f64),
            ("gc_runs", st.gc_runs as f64),
            ("records_written", st.records_written as f64),
            ("record_bytes_written", st.record_bytes_written as f64),
            ("free_sectors", self.store.free_sectors() as f64),
        ] {
            extra.insert(k.to_string(), v);
        }
        CandidateReport {
            ram_bytes: (st.index_ram_bytes + st.tree_ram_bytes + st.sector_table_ram_bytes) as u64,
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
            .with_dial("codec", "stored")
            .with_dial("record_max", "512")
            .with_dial("gc_policy", "greedy");
        let c = tree_store_config(&cfg).unwrap();
        assert_eq!(c.codec, Codec::Stored);
        assert_eq!(c.record_max, 512);
        assert!(tree_store_config(&CandidateConfig::new(32).with_dial("nope", "1")).is_err());
    }

    #[test]
    fn a_small_sweep_has_no_failures() {
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
            &CandidateConfig::new(32),
            &wl,
            &params,
            &Scoreboard::memory(),
        );
        assert!(
            out.iter().all(|s| s.cases > 0 && s.failures == 0),
            "{out:?}"
        );
    }
}
