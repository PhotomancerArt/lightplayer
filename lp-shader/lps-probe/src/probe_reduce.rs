//! [`ProbeReduce`]: how per-site probe values are aggregated.

use serde::{Deserialize, Serialize};

/// Reduction applied to a probe's per-site values, per vary step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeReduce {
    /// Raw value rows (capped at `MAX_RAW_VALUES` total).
    #[default]
    None,
    /// Per-component min/max/mean plus NaN/Inf counts.
    Stats,
    /// A histogram over all components pooled, with `bins` buckets between
    /// the observed finite min and max.
    Histogram { bins: u8 },
}
