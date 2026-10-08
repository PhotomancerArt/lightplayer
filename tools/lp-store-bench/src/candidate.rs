//! The one interface every store candidate implements.
//!
//! A [`Candidate`] is a factory (format, mount); a mount yields a
//! [`CandidateStore`], the live store. Object-safe on purpose, so the drivers
//! can hold a list of candidates and fan out over them.

use std::collections::BTreeMap;

use lp_nor_sim::{NorFlashSim, NorGeometry};
use serde::{Deserialize, Serialize};

use crate::{CandidateReport, StoreError};

/// Geometry plus the candidate's own dials (an opaque string map).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateConfig {
    pub sectors: u32,
    #[serde(default)]
    pub dials: BTreeMap<String, String>,
}

impl CandidateConfig {
    pub fn new(sectors: u32) -> Self {
        Self {
            sectors,
            dials: BTreeMap::new(),
        }
    }

    pub fn with_dial(mut self, key: &str, value: &str) -> Self {
        self.dials.insert(key.into(), value.into());
        self
    }

    pub fn geometry(&self) -> NorGeometry {
        NorGeometry::c6(self.sectors)
    }

    pub fn dial(&self, key: &str) -> Option<&str> {
        self.dials.get(key).map(String::as_str)
    }

    /// A dial parsed as a number, or `default`.
    pub fn dial_u32(&self, key: &str, default: u32) -> u32 {
        self.dial(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    /// `k=v+k=v`, the form used in candidate specs (`t1@record_max=512+codec=stored`).
    pub fn dials_label(&self) -> String {
        self.dials
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("+")
    }
}

/// A store design under test: how to lay it down and how to open it.
pub trait Candidate: Send + Sync {
    /// Short id used on the command line and in the scoreboard (`f1`, `t1`, …).
    fn name(&self) -> &str;
    /// Lay a fresh, empty store onto `flash` (erased or garbage).
    fn format(&self, flash: &mut NorFlashSim, cfg: &CandidateConfig) -> Result<(), StoreError>;
    /// Open the store after a power cycle; may repair. Must never panic on any
    /// flash content (the harness catches panics and scores them as failures).
    fn mount(
        &self,
        flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Box<dyn CandidateStore>, (StoreError, NorFlashSim)>;
}

/// A mounted store.
pub trait CandidateStore {
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError>;
    fn get(&mut self, path: &str) -> Result<Option<Vec<u8>>, StoreError>;
    /// Delete every path that starts with `prefix`.
    fn delete_prefix(&mut self, prefix: &str) -> Result<(), StoreError>;
    /// Every path that starts with `prefix`, sorted.
    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StoreError>;
    /// End of a workload step. A transactional store makes everything since the
    /// last commit atomic; others may no-op.
    fn commit(&mut self) -> Result<(), StoreError>;
    /// Unmount (without writing anything new) and hand the flash back.
    fn into_flash(self: Box<Self>) -> NorFlashSim;
    /// A clone of the flash as it stands (cheap: sectors are shared), for
    /// counters and measures. Never written back.
    fn flash_snapshot(&self) -> NorFlashSim;
    fn report(&self) -> CandidateReport;
}
