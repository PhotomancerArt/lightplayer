//! Compare two compiled shaders over the health sites of one spec.

use alloc::format;
use alloc::string::String;

use lps_shared::LpsValueF32;
use serde::{Deserialize, Serialize};

use crate::experiment::apply_bindings;
use crate::experiment_result::round_sig4;
use crate::experiment_spec::ExperimentSpec;
use crate::health::health_sites;
use crate::interp_harness::CompiledShader;

/// Per-site render deltas over sites with delta > this count as "changed".
const CHANGE_EPSILON: f32 = 1e-6;

/// Summary of how `render` output differs between two compiled shaders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiffReport {
    /// Largest per-site delta (max abs component difference; infinity when
    /// exactly one side is non-finite at a site).
    pub max_delta: f32,
    /// Mean per-site delta.
    pub mean_delta: f32,
    /// Fraction of sites with delta > 1e-6.
    pub changed_fraction: f32,
    /// Normalized `[x0, y0, x1, y1]` bounding box of changed sites, if any.
    pub changed_bbox: Option<[f32; 4]>,
}

impl DiffReport {
    /// Round result floats to 4 significant digits (transport helper).
    pub fn rounded(mut self) -> Self {
        self.max_delta = round_sig4(self.max_delta);
        self.mean_delta = round_sig4(self.mean_delta);
        self.changed_fraction = round_sig4(self.changed_fraction);
        if let Some(b) = &mut self.changed_bbox {
            for x in b.iter_mut() {
                *x = round_sig4(*x);
            }
        }
        self
    }
}

/// Evaluate `render` of both shaders over the health sites (health grid +
/// LED points) under `spec.bindings` (no vary) and summarize the deltas.
pub fn diff_experiments(
    prev: &CompiledShader,
    cur: &CompiledShader,
    spec: &ExperimentSpec,
) -> Result<DiffReport, String> {
    let mut a = prev
        .instantiate()
        .map_err(|e| format!("diff: prev instantiate: {e}"))?;
    let mut b = cur
        .instantiate()
        .map_err(|e| format!("diff: cur instantiate: {e}"))?;
    apply_bindings(&mut a, spec, None);
    apply_bindings(&mut b, spec, None);

    let sites = health_sites(spec);
    let mut max_delta = 0.0f32;
    let mut delta_sum = 0.0f64;
    let mut changed = 0u32;
    let mut bbox: Option<[f32; 4]> = None;

    for site in &sites {
        let pos = [site[0] * spec.size[0] as f32, site[1] * spec.size[1] as f32];
        let va = call_render(&mut a, pos, "prev")?;
        let vb = call_render(&mut b, pos, "cur")?;
        let delta = site_delta(&va, &vb);
        if delta > max_delta || (delta.is_nan() && !max_delta.is_nan()) {
            max_delta = delta;
        }
        delta_sum += f64::from(delta);
        if delta > CHANGE_EPSILON {
            changed += 1;
            bbox = Some(match bbox {
                None => [site[0], site[1], site[0], site[1]],
                Some([x0, y0, x1, y1]) => [
                    x0.min(site[0]),
                    y0.min(site[1]),
                    x1.max(site[0]),
                    y1.max(site[1]),
                ],
            });
        }
    }

    let n = sites.len().max(1) as f64;
    Ok(DiffReport {
        max_delta,
        mean_delta: (delta_sum / n) as f32,
        changed_fraction: f64::from(changed) as f32 / n as f32,
        changed_bbox: bbox,
    })
}

fn call_render(
    inst: &mut crate::interp_harness::InterpInstance,
    pos: [f32; 2],
    which: &str,
) -> Result<[f32; 4], String> {
    match inst.call("render_2d", &[LpsValueF32::Vec2(pos)]) {
        Ok(LpsValueF32::Vec4(v)) => Ok(v),
        Ok(other) => Err(format!(
            "diff: {which} render returned {other:?}, expected vec4"
        )),
        Err(e) => Err(format!("diff: {which} render failed: {e}")),
    }
}

/// Max abs component difference. Identical values (including equal
/// infinities, and NaN on both sides) contribute 0; a non-finite value on
/// one side only contributes infinity.
fn site_delta(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let mut delta = 0.0f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        let d = component_delta(x, y);
        if d > delta {
            delta = d;
        }
    }
    delta
}

fn component_delta(x: f32, y: f32) -> f32 {
    if x == y || (x.is_nan() && y.is_nan()) {
        0.0
    } else if !x.is_finite() || !y.is_finite() {
        f32::INFINITY
    } else {
        (x - y).abs()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    fn compile(src: &str) -> CompiledShader {
        CompiledShader::compile(src).expect("compile")
    }

    #[test]
    fn identical_sources_diff_to_zero() {
        let src = "vec4 render_2d(vec2 pos) { return vec4(pos.x / 256.0, 0.5, 0.25, 1.0); }";
        let a = compile(src);
        let b = compile(src);
        let report = diff_experiments(&a, &b, &ExperimentSpec::default()).expect("diff");
        assert_eq!(report.max_delta, 0.0);
        assert_eq!(report.mean_delta, 0.0);
        assert_eq!(report.changed_fraction, 0.0);
        assert_eq!(report.changed_bbox, None);
    }

    #[test]
    fn localized_edit_yields_sensible_bbox() {
        let a = compile("vec4 render_2d(vec2 pos) { return vec4(0.0, 0.0, 0.0, 1.0); }");
        let b = compile(
            "vec4 render_2d(vec2 pos) {\n    if (pos.x > 192.0) { return vec4(1.0, 0.0, 0.0, 1.0); }\n    return vec4(0.0, 0.0, 0.0, 1.0);\n}",
        );
        let report = diff_experiments(&a, &b, &ExperimentSpec::default()).expect("diff");
        assert_eq!(report.max_delta, 1.0);
        // Health grid columns with center x·256 > 192: 4 of 16 → 25% changed.
        assert_eq!(report.changed_fraction, 0.25);
        let bbox = report.changed_bbox.expect("bbox");
        assert!(bbox[0] > 0.75 && bbox[0] < 0.8, "bbox: {bbox:?}");
        assert!(bbox[2] > 0.95, "bbox: {bbox:?}");
        assert!(bbox[1] < 0.05 && bbox[3] > 0.95, "bbox: {bbox:?}");
    }

    #[test]
    fn one_sided_nan_is_infinite_delta() {
        let a = compile(
            "layout(binding = 0) uniform float z;\nvec4 render_2d(vec2 pos) { return vec4(z / z, 0.0, 0.0, 1.0); }",
        );
        let b = compile("vec4 render_2d(vec2 pos) { return vec4(0.5, 0.0, 0.0, 1.0); }");
        let report = diff_experiments(&a, &b, &ExperimentSpec::default()).expect("diff");
        assert_eq!(report.max_delta, f32::INFINITY);
        assert_eq!(report.changed_fraction, 1.0);
        let _: Vec<f32> = report.changed_bbox.expect("bbox").into();
    }
}
