//! **The engine source** (D2): where the host gets an engine it must hold —
//! the board's *current* engine before an update (the backup), or the
//! engine a heal installs. A small sans-IO machine: it asks for effects and
//! takes their results.
//!
//! ```text
//! need <sha, target, build_id>
//!   → LookUpCache { sha }                        → Some(bytes) | None
//!   → FetchFromStore { target, build_id, sha }   → Found(bytes) | NotFound | Offline
//!   → (a backup only: the engine runs or crashes) read it back from the board
//!   → every source's bytes are verified against <sha> (the engine hash rule)
//!   → KeepInCache { sha, bytes }  (after any source but the cache)
//!   → Held(bytes) | Missing (E13)
//! ```
//!
//! The order is cache → store → read-back: the read-back is the rare
//! fallback. **These are effects, not dependencies:** this crate never
//! depends on the firmware distribution's store; the edge resolves
//! `LookUpCache` with its engine cache and `FetchFromStore` with
//! `lpa_firmware_store::fetch_engine_from_store` (lp-cli, Studio in M7). A
//! heal never reads back: there is no engine to read.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_update::hash_rules::engine_sha256;

/// Something only the edge can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceEffect {
    LookUpCache {
        sha: [u8; 32],
    },
    FetchFromStore {
        target: String,
        build_id: String,
        sha: [u8; 32],
    },
    /// Read the engine back from the board (the update driver does this
    /// itself, over its link).
    ReadBack {
        sha: [u8; 32],
        len: u32,
    },
    /// Keep a verified engine (always, after a source other than the cache).
    KeepInCache {
        sha: [u8; 32],
        bytes: Vec<u8>,
    },
}

/// What the store said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreAnswer {
    Found(Vec<u8>),
    NotFound,
    Offline,
}

/// The result of the effect the source last asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceResult {
    Cache(Option<Vec<u8>>),
    Store(StoreAnswer),
    ReadBack(Option<Vec<u8>>),
}

/// Where the source stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceStep {
    /// Perform this and pass its result to [`EngineSource::on_result`].
    Ask(SourceEffect),
    /// The engine, verified. `keep` is the `KeepInCache` to perform, unless
    /// it came from the cache.
    Held {
        bytes: Vec<u8>,
        keep: Option<SourceEffect>,
    },
    /// No source had it (E13). `offline` when the store could not be asked.
    Missing { offline: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Cache,
    Store,
    ReadBack,
}

/// One engine to find. See the module docs.
#[derive(Clone, Debug)]
pub struct EngineSource {
    sha: [u8; 32],
    target: String,
    build_id: String,
    /// The engine's length when a read-back is allowed (a backup of a
    /// running or crashing engine); `None` for a heal.
    read_back_len: Option<u32>,
    stage: Stage,
    offline: bool,
}

impl EngineSource {
    /// Find the engine `sha` of `build_id` (a `target` build). With
    /// `read_back_len`, the board's own engine may be read back last.
    #[must_use]
    pub fn new(
        sha: [u8; 32],
        target: String,
        build_id: String,
        read_back_len: Option<u32>,
    ) -> Self {
        Self {
            sha,
            target,
            build_id,
            read_back_len,
            stage: Stage::Cache,
            offline: false,
        }
    }

    /// The first effect.
    #[must_use]
    pub fn start(&self) -> SourceStep {
        SourceStep::Ask(SourceEffect::LookUpCache { sha: self.sha })
    }

    /// The result of the effect last asked for.
    pub fn on_result(&mut self, result: SourceResult) -> SourceStep {
        let (bytes, from_cache) = match (self.stage, result) {
            (Stage::Cache, SourceResult::Cache(bytes)) => (bytes, true),
            (Stage::Store, SourceResult::Store(answer)) => match answer {
                StoreAnswer::Found(bytes) => (Some(bytes), false),
                StoreAnswer::NotFound => (None, false),
                StoreAnswer::Offline => {
                    self.offline = true;
                    (None, false)
                }
            },
            (Stage::ReadBack, SourceResult::ReadBack(bytes)) => (bytes, false),
            // A result for a step this source is not at: ask again.
            _ => return self.ask(),
        };
        if let Some(bytes) = bytes.filter(|b| engine_sha256(b) == self.sha) {
            let keep = (!from_cache).then(|| SourceEffect::KeepInCache {
                sha: self.sha,
                bytes: bytes.clone(),
            });
            return SourceStep::Held { bytes, keep };
        }
        self.stage = match self.stage {
            Stage::Cache => Stage::Store,
            Stage::Store if self.read_back_len.is_some() => Stage::ReadBack,
            Stage::Store | Stage::ReadBack => {
                return SourceStep::Missing {
                    offline: self.offline,
                };
            }
        };
        self.ask()
    }

    fn ask(&self) -> SourceStep {
        SourceStep::Ask(match self.stage {
            Stage::Cache => SourceEffect::LookUpCache { sha: self.sha },
            Stage::Store => SourceEffect::FetchFromStore {
                target: self.target.clone(),
                build_id: self.build_id.clone(),
                sha: self.sha,
            },
            Stage::ReadBack => SourceEffect::ReadBack {
                sha: self.sha,
                len: self.read_back_len.unwrap_or(0),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn engine() -> (Vec<u8>, [u8; 32]) {
        let e = vec![5u8; 9000];
        let sha = engine_sha256(&e);
        (e, sha)
    }

    fn source(read_back: bool) -> EngineSource {
        let (e, sha) = engine();
        EngineSource::new(
            sha,
            "esp32c6-4mb".into(),
            "v+abc".into(),
            read_back.then_some(e.len() as u32),
        )
    }

    #[test]
    fn the_cache_first_and_nothing_to_keep() {
        let (e, _) = engine();
        let mut s = source(true);
        assert!(matches!(
            s.start(),
            SourceStep::Ask(SourceEffect::LookUpCache { .. })
        ));
        assert_eq!(
            s.on_result(SourceResult::Cache(Some(e.clone()))),
            SourceStep::Held {
                bytes: e,
                keep: None
            }
        );
    }

    #[test]
    fn then_the_store_and_kept() {
        let (e, sha) = engine();
        let mut s = source(false);
        assert!(matches!(
            s.on_result(SourceResult::Cache(None)),
            SourceStep::Ask(SourceEffect::FetchFromStore { .. })
        ));
        assert_eq!(
            s.on_result(SourceResult::Store(StoreAnswer::Found(e.clone()))),
            SourceStep::Held {
                bytes: e.clone(),
                keep: Some(SourceEffect::KeepInCache { sha, bytes: e })
            }
        );
    }

    #[test]
    fn bytes_that_do_not_hash_never_count_as_held() {
        let (e, _) = engine();
        let mut s = source(true);
        s.on_result(SourceResult::Cache(Some(vec![1, 2, 3])));
        let step = s.on_result(SourceResult::Store(StoreAnswer::Found(vec![9; 9000])));
        assert!(matches!(
            step,
            SourceStep::Ask(SourceEffect::ReadBack { len: 9000, .. })
        ));
        assert!(matches!(
            s.on_result(SourceResult::ReadBack(Some(e))),
            SourceStep::Held { .. }
        ));
    }

    #[test]
    fn e13_a_heal_with_no_source_is_missing_and_never_reads_back() {
        let mut s = source(false);
        s.on_result(SourceResult::Cache(None));
        assert_eq!(
            s.on_result(SourceResult::Store(StoreAnswer::Offline)),
            SourceStep::Missing { offline: true }
        );
        let mut s = source(true);
        s.on_result(SourceResult::Cache(None));
        s.on_result(SourceResult::Store(StoreAnswer::NotFound));
        assert_eq!(
            s.on_result(SourceResult::ReadBack(None)),
            SourceStep::Missing { offline: false }
        );
    }
}
