//! [`ExperimentResult`]: diagnostics, health, and per-probe outcomes.

use alloc::string::String;
use alloc::vec::Vec;

use lp_collection::VecMap;
use serde::{Deserialize, Serialize};

use crate::health::HealthReport;
use crate::interp_harness::CompiledShader;

/// Everything one [`crate::run_experiment`] call produced.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExperimentResult {
    pub shader: ShaderCompileOutcome,
    /// The compiled module on success (cacheable for [`crate::diff`]);
    /// not part of the JSON form.
    #[serde(skip)]
    pub compiled: Option<CompiledShader>,
    /// Present iff the shader compiled.
    pub health: Option<HealthReport>,
    /// One outcome per probe id, in spec order.
    pub probes: VecMap<String, ProbeOutcome>,
    /// Non-fatal notes (e.g. bindings that don't match a declared uniform).
    pub warnings: Vec<String>,
}

impl ExperimentResult {
    /// Round every result float to 4 significant digits for compact JSON
    /// transport. Applies to results only — specs are never rounded.
    pub fn rounded(mut self) -> Self {
        if let Some(h) = &mut self.health {
            h.near_black_fraction = round_sig4(h.near_black_fraction);
            h.mean_luminance = round_sig4(h.mean_luminance);
            h.clip_fraction = round_sig4(h.clip_fraction);
        }
        for (_, outcome) in self.probes.iter_mut() {
            outcome.round_floats();
        }
        self
    }
}

/// Compile outcome for the bare user shader (its diagnostics are the
/// user-meaningful ones; probe-wrapper diagnostics are reported per probe).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderCompileOutcome {
    Ok,
    Err { diagnostics: Vec<String> },
}

/// Result of one probe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcome {
    /// Raw rows (`reduce: none`).
    Values(ProbeValues),
    /// Per-component statistics per vary step (`reduce: stats`).
    Stats(ProbeStats),
    /// Pooled-component histogram per vary step (`reduce: histogram`).
    Histogram(ProbeHistogram),
    /// The probe wrapper failed to compile; diagnostics are attributed to
    /// the probe expression (expr-relative positions).
    CompileError { diagnostics: Vec<String> },
    /// The probe was not evaluated; the reason is actionable.
    Skipped { reason: String },
}

impl ProbeOutcome {
    fn round_floats(&mut self) {
        match self {
            ProbeOutcome::Values(v) => {
                for row in &mut v.rows {
                    if let Some(x) = &mut row.vary {
                        *x = round_sig4(*x);
                    }
                    for x in row.at.iter_mut().chain(row.value.iter_mut()) {
                        *x = round_sig4(*x);
                    }
                }
            }
            ProbeOutcome::Stats(s) => {
                for step in &mut s.steps {
                    if let Some(x) = &mut step.vary {
                        *x = round_sig4(*x);
                    }
                    for x in step
                        .min
                        .iter_mut()
                        .chain(step.max.iter_mut())
                        .chain(step.mean.iter_mut())
                    {
                        *x = round_sig4(*x);
                    }
                }
            }
            ProbeOutcome::Histogram(h) => {
                for step in &mut h.steps {
                    if let Some(x) = &mut step.vary {
                        *x = round_sig4(*x);
                    }
                    step.lo = round_sig4(step.lo);
                    step.hi = round_sig4(step.hi);
                }
            }
            ProbeOutcome::CompileError { .. } | ProbeOutcome::Skipped { .. } => {}
        }
    }
}

/// Raw probe rows in evaluation order (vary outer, sites inner).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeValues {
    pub rows: Vec<ProbeValueRow>,
}

/// One evaluated site.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeValueRow {
    /// The vary step's value, when a `vary` was given.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub vary: Option<f32>,
    /// Site coordinates: `[x, y]` normalized for position domains, `[t]`
    /// for sweep domains.
    pub at: Vec<f32>,
    /// Value components (ints are reported as f32).
    pub value: Vec<f32>,
}

/// Per-vary-step component statistics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeStats {
    pub steps: Vec<ProbeStepStats>,
}

/// Statistics over all sites of one vary step. `min`/`max`/`mean` are
/// per-component and computed over finite values only; non-finite values
/// are counted in `nan_count`/`inf_count` instead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeStepStats {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub vary: Option<f32>,
    pub min: Vec<f32>,
    pub max: Vec<f32>,
    pub mean: Vec<f32>,
    pub nan_count: u32,
    pub inf_count: u32,
}

/// Per-vary-step histograms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeHistogram {
    pub steps: Vec<ProbeStepHistogram>,
}

/// Histogram over all components of all sites of one vary step, pooled.
/// `counts` has `bins` buckets spanning `[lo, hi]` (the observed finite
/// range); non-finite components are counted separately.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeStepHistogram {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub vary: Option<f32>,
    pub lo: f32,
    pub hi: f32,
    pub counts: Vec<u32>,
    pub non_finite_count: u32,
}

/// Round `x` to 4 significant digits (result serialization helper).
/// Non-finite values and zero pass through unchanged.
pub fn round_sig4(x: f32) -> f32 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let exp = libm::floorf(libm::log10f(libm::fabsf(x)));
    let scale = libm::powf(10.0, 3.0 - exp);
    libm::roundf(x * scale) / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_sig4_keeps_4_significant_digits() {
        assert_eq!(round_sig4(0.123_456), 0.123_5);
        assert_eq!(round_sig4(123_456.0), 123_500.0);
        assert_eq!(round_sig4(-0.001_234_49), -0.001_234);
        assert_eq!(round_sig4(0.0), 0.0);
        assert_eq!(round_sig4(1.0), 1.0);
        assert!(round_sig4(f32::NAN).is_nan());
        assert_eq!(round_sig4(f32::INFINITY), f32::INFINITY);
    }

    #[test]
    fn result_serializes_without_compiled_module() {
        let result = ExperimentResult {
            shader: ShaderCompileOutcome::Ok,
            compiled: None,
            health: None,
            probes: VecMap::new(),
            warnings: Vec::new(),
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(!json.contains("compiled"), "{json}");
        let back: ExperimentResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.shader, ShaderCompileOutcome::Ok);
        assert!(back.compiled.is_none());
    }
}
