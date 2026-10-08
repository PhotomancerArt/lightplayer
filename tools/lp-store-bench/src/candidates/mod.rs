//! The candidates, by name.

mod mem_candidate;

pub use mem_candidate::{MemCandidate, MemLayout};

use crate::{Candidate, CandidateConfig};

/// Every candidate name the CLI accepts.
pub const CANDIDATE_NAMES: &[&str] = &["mem", "mem-broken"];

/// A candidate by its short name.
pub fn candidate_by_name(name: &str) -> Option<Box<dyn Candidate>> {
    Some(match name {
        "mem" => Box::new(MemCandidate::new(MemLayout::PingPong)),
        "mem-broken" => Box::new(MemCandidate::new(MemLayout::InPlace)),
        _ => return None,
    })
}

/// `name[@k=v+k=v]` → (candidate, config at `sectors`).
pub fn parse_candidate_spec(
    spec: &str,
    sectors: u32,
) -> Result<(Box<dyn Candidate>, CandidateConfig), String> {
    let (name, dials) = spec.split_once('@').unwrap_or((spec, ""));
    let cand = candidate_by_name(name).ok_or_else(|| {
        format!(
            "unknown candidate {name:?} (known: {})",
            CANDIDATE_NAMES.join(", ")
        )
    })?;
    let mut cfg = CandidateConfig::new(sectors);
    for kv in dials.split('+').filter(|s| !s.is_empty()) {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| format!("bad dial {kv:?}"))?;
        if k == "sectors" {
            cfg.sectors = v.parse().map_err(|_| format!("bad sectors {v:?}"))?;
        } else {
            cfg.dials.insert(k.into(), v.into());
        }
    }
    Ok((cand, cfg))
}
