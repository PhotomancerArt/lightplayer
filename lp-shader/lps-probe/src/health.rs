//! Always-on shader health evaluation.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::LpsValueF32;
use serde::{Deserialize, Serialize};

use crate::experiment_spec::ExperimentSpec;
use crate::interp_harness::InterpInstance;
use crate::probe_domain::grid_sites;

/// Health grid resolution (per axis); health evaluates `render` at these
/// grid cell centers plus every LED point.
pub const HEALTH_GRID: u16 = 16;

/// Always-on `render` health summary over the health grid + LED points,
/// under the experiment bindings (no vary).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    /// NaN components across all evaluated sites (r/g/b/a counted separately).
    pub nan_count: u32,
    /// Infinite components across all evaluated sites.
    pub inf_count: u32,
    /// Fraction of evaluated sites with finite luminance < 0.01.
    pub near_black_fraction: f32,
    /// Mean luminance (Rec. 709 weights) over sites with finite luminance.
    pub mean_luminance: f32,
    /// Fraction of evaluated sites with any component > 1 or < 0.
    pub clip_fraction: f32,
    /// Sites successfully evaluated (health grid + LED points).
    pub sites_evaluated: u32,
}

/// The normalized health sites for `spec`: the health grid plus LED points.
pub(crate) fn health_sites(spec: &ExperimentSpec) -> Vec<[f32; 2]> {
    let mut sites = grid_sites(HEALTH_GRID, HEALTH_GRID, [0.0, 0.0, 1.0, 1.0]);
    sites.extend(spec.led_points.iter().map(|p| p.at));
    sites
}

/// Consecutive evaluation failures after which health evaluation stops.
/// Failures are rarely site-specific (an exhausted op budget or a broken
/// import fails everywhere), and each op-budget exhaustion burns the full
/// per-evaluation fuel — bailing keeps a hostile shader's health pass cheap.
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Evaluate the health report on an already-bound instance. Evaluation
/// failures are reported as warnings and excluded from `sites_evaluated`;
/// after [`MAX_CONSECUTIVE_FAILURES`] failures in a row the remaining sites
/// are abandoned (with a warning).
pub(crate) fn evaluate_health(
    instance: &mut InterpInstance,
    spec: &ExperimentSpec,
    warnings: &mut Vec<String>,
) -> HealthReport {
    let mut nan_count = 0u32;
    let mut inf_count = 0u32;
    let mut near_black = 0u32;
    let mut lum_sum = 0.0f64;
    let mut lum_sites = 0u32;
    let mut clipped = 0u32;
    let mut evaluated = 0u32;
    let mut consecutive_failures = 0u32;

    for site in health_sites(spec) {
        let pos = [site[0] * spec.size[0] as f32, site[1] * spec.size[1] as f32];
        let rgba = match instance.call("render_2d", &[LpsValueF32::Vec2(pos)]) {
            Ok(LpsValueF32::Vec4(v)) => v,
            Ok(other) => {
                warnings.push(format!("health: render returned {other:?}, expected vec4"));
                continue;
            }
            Err(e) => {
                warnings.push(format!(
                    "health: render failed at ({:.4}, {:.4}): {e}",
                    site[0], site[1]
                ));
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    warnings.push(format!(
                        "health: abandoned after {consecutive_failures} consecutive \
                         evaluation failures"
                    ));
                    break;
                }
                continue;
            }
        };
        consecutive_failures = 0;
        evaluated += 1;
        for c in rgba {
            if c.is_nan() {
                nan_count += 1;
            } else if c.is_infinite() {
                inf_count += 1;
            }
        }
        if rgba.iter().any(|&c| c > 1.0 || c < 0.0) {
            clipped += 1;
        }
        let lum = 0.2126 * rgba[0] + 0.7152 * rgba[1] + 0.0722 * rgba[2];
        if lum.is_finite() {
            lum_sites += 1;
            lum_sum += f64::from(lum);
            if lum < 0.01 {
                near_black += 1;
            }
        }
    }

    HealthReport {
        nan_count,
        inf_count,
        near_black_fraction: fraction(near_black, evaluated),
        mean_luminance: if lum_sites > 0 {
            (lum_sum / f64::from(lum_sites)) as f32
        } else {
            0.0
        },
        clip_fraction: fraction(clipped, evaluated),
        sites_evaluated: evaluated,
    }
}

fn fraction(part: u32, whole: u32) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}
